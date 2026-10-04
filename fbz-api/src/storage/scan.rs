use super::{db, importer, rpc, sql_error};
use crate::{error::AppError, state::AppState};
use serde_json::{Value, json};
use sqlx::Row;

pub fn spawn_worker(state: AppState) {
    tokio::spawn(async move {
        loop {
            let delay = match step(&state).await {
                Ok(true) => 50,
                Ok(false) => 2000,
                Err(error) => {
                    tracing::warn!(message=?error,"cloud storage worker failed");
                    2000
                }
            };
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }
    });
}
pub async fn step(state: &AppState) -> Result<bool, AppError> {
    sqlx::query("update storage_mounts m set status=case when m.status='failed' then case when jsonb_array_length(frontier)>0 then 'scanning' else 'importing' end else 'scanning' end,generation=case when m.status='failed' then generation else generation+1 end,frontier=case when m.status='failed' then frontier else jsonb_build_array(jsonb_build_object('id',root_id,'page',0)) end,import_cursor=case when m.status='failed' then import_cursor else '' end,scanned=case when m.status='failed' then scanned else 0 end,imported=case when m.status='failed' then imported else 0 end,last_error=null,updated_at=now() where m.id in(select s.id from storage_mounts s join storage_accounts a on a.id=s.account_id where s.library_id is not null and s.refresh_minutes>0 and s.next_refresh_at<=now() and s.status in ('idle','failed') and a.status='ready' for update of s skip locked limit 1)").execute(db(state)?).await.map_err(sql_error)?;
    let mut tx = db(state)?.begin().await.map_err(sql_error)?;
    let row=sqlx::query("select m.*,m.id::text as mount_key,m.account_id::text as account_key,l.library_type from storage_mounts m join libraries l on l.id=m.library_id where m.status in ('scanning','importing') order by m.updated_at for update of m skip locked limit 1")
        .fetch_optional(&mut *tx).await.map_err(sql_error)?;
    let Some(row) = row else {
        return Ok(false);
    };
    let id: String = row.get("mount_key");
    let account: String = row.get("account_key");
    let generation: i64 = row.get("generation");
    let result:Result<(),AppError>=async {
        if row.get::<String,_>("status")=="scanning"{
            let mut frontier:Vec<Value>=serde_json::from_value(row.get("frontier")).map_err(|_|AppError::internal("扫描断点无效"))?;
            if frontier.is_empty(){return Err(AppError::internal("扫描目录队列为空"));}
            let current=frontier.remove(0);let parent=current["id"].as_str().unwrap_or("");
            let result=rpc(state,&account,json!({"op":"list","parentId":parent,"page":current["page"]})).await?;
            let entries=result["entries"].as_array().ok_or_else(||AppError::unprocessable("云端目录响应无效"))?;
            for entry in entries {
                let file_id=entry["id"].as_str().ok_or_else(||AppError::unprocessable("云端文件 ID 无效"))?;
                let name=entry["name"].as_str().ok_or_else(||AppError::unprocessable("云端文件名无效"))?;
                if entry["directory"]==true {
                    let seen:bool=sqlx::query_scalar("select exists(select 1 from storage_entries where mount_id::text=$1 and remote_id=$2 and generation=$3)").bind(&id).bind(file_id).bind(generation).fetch_one(&mut *tx).await.map_err(sql_error)?;
                    if !seen && file_id!=row.get::<String,_>("root_id") {frontier.push(json!({"id":file_id,"page":0}));}
                }
                sqlx::query("insert into storage_entries(mount_id,remote_id,parent_id,name,entry,generation) values($1::uuid,$2,$3,$4,$5,$6) on conflict(mount_id,remote_id) do update set parent_id=excluded.parent_id,name=excluded.name,entry=excluded.entry,generation=excluded.generation")
                    .bind(&id).bind(file_id).bind(parent).bind(name).bind(entry).bind(generation).execute(&mut *tx).await.map_err(sql_error)?;
            }
            if let Some(page)=result["nextPage"].as_u64(){if page>100000{return Err(AppError::unprocessable("目录分页超过上限"));}frontier.insert(0,json!({"id":parent,"page":page}));}
            if frontier.len()>100000{return Err(AppError::unprocessable("目录队列超过上限，请拆分挂载"));}
            sqlx::query("update storage_mounts set frontier=$2,status=$3,scanned=scanned+$4,updated_at=now() where id::text=$1").bind(&id).bind(json!(frontier)).bind(if frontier.is_empty(){"importing"}else{"scanning"}).bind(entries.len() as i64).execute(&mut *tx).await.map_err(sql_error)?;
        }else{
            let cursor:String=row.get("import_cursor");
            let entry=sqlx::query("select remote_id,entry,parent_id,media_item_id,imported_version from storage_entries where mount_id::text=$1 and generation=$2 and remote_id>$3 and coalesce((entry->>'directory')::boolean,false)=false and lower(entry->>'name') ~ '[.](mp4|mkv|m4v|mov|avi|ts|m2ts|webm)$' order by remote_id limit 1")
                .bind(&id).bind(generation).bind(cursor).fetch_optional(&mut *tx).await.map_err(sql_error)?;
            if let Some(entry)=entry{
                let value:Value=entry.get("entry");let remote:String=entry.get("remote_id");
                let imported=if importer::is_video(value["name"].as_str().unwrap_or("")) {
                    importer::import_video(state,&mut tx,&id,&account,row.get("library_id"),&row.get::<String,_>("library_type"),&entry.get::<String,_>("parent_id"),&value,generation).await?;1_i64
                }else{0};
                sqlx::query("update storage_mounts set import_cursor=$2,imported=imported+$3,updated_at=now() where id::text=$1").bind(&id).bind(remote).bind(imported).execute(&mut *tx).await.map_err(sql_error)?;
            }else{
                // Reconcile only after a complete listing AND successful import of every entry.
                sqlx::query("update media_items set is_deleted=true,scan_status='missing' where id in(select media_item_id from storage_entries where mount_id::text=$1 and generation<>$2 and media_file_id is not null)").bind(&id).bind(generation).execute(&mut *tx).await.map_err(sql_error)?;
                for kind in ["season", "series"] {
                    sqlx::query("update media_items mi set is_deleted=true where mi.item_type=$2 and mi.id in (select media_item_id from storage_groups where mount_id::text=$1) and not exists(select 1 from media_items child where child.parent_id=mi.id and child.is_deleted=false)").bind(&id).bind(kind).execute(&mut *tx).await.map_err(sql_error)?;
                }
                sqlx::query("update storage_mounts set status='idle',last_error=null,last_refreshed_at=now(),failure_count=0,next_refresh_at=now()+greatest(refresh_minutes,5)*interval '1 minute',updated_at=now() where id::text=$1").bind(&id).execute(&mut *tx).await.map_err(sql_error)?;
            }
        }Ok(())
    }.await;
    match result {
        Ok(()) => tx.commit().await.map(|_| true).map_err(sql_error),
        Err(error) => {
            tx.rollback().await.map_err(sql_error)?;
            sqlx::query("update storage_mounts set status='failed',last_error=$2,failure_count=least(failure_count+1,10),next_refresh_at=now()+least(1440,greatest(refresh_minutes,5)*power(2,least(failure_count,8))) * interval '1 minute',updated_at=now() where id::text=$1").bind(&id).bind(error.message()).execute(db(state)?).await.map_err(sql_error)?;
            Ok(true)
        }
    }
}
