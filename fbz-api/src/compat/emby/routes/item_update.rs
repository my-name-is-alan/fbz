//! Jellyfin `POST /Items/{itemId}`（元数据编辑器保存，UpdateItem）。
//!
//! 接收 BaseItemDto 子集（PascalCase）：Name / OriginalTitle / ForcedSortName / Overview /
//! ProductionYear / PremiereDate / OfficialRating / CommunityRating / CriticRating /
//! Genres / Tags / Studios[{Name}] / People[{Name,Role,Type}] / ProviderIds。
//! 写入复用插件 Host 的校验与事务（`plugins::host::apply_item_metadata_patch`），条目标记 manual。
//! `NFO_SAVER_ENABLED=true` 时随后写回 NFO（见 `metadata::nfo_write`）。
use std::collections::BTreeMap;

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, Uri},
};
use serde::Deserialize;

use crate::{
    compat::emby::payload::parse_emby_body,
    error::AppError,
    plugins::host::{PatchPluginItemExternalIdDto, PatchPluginItemMetadataRequestDto, PatchPluginItemPersonDto, apply_item_metadata_patch},
    state::AppState,
};

use super::access::authenticate_request_user;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct UpdateItemDto {
    pub name: Option<String>,
    pub original_title: Option<String>,
    pub forced_sort_name: Option<String>,
    pub sort_name: Option<String>,
    pub overview: Option<String>,
    pub production_year: Option<i32>,
    pub premiere_date: Option<String>,
    pub official_rating: Option<String>,
    pub community_rating: Option<f64>,
    pub critic_rating: Option<f64>,
    pub genres: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub studios: Option<Vec<NameDto>>,
    pub people: Option<Vec<PersonDto>>,
    pub provider_ids: Option<BTreeMap<String, Option<String>>>,
    pub lock_data: Option<bool>,
    pub locked_fields: Option<Vec<String>>,
}

/// JF `MetadataField` 枚举（LockedFields 合法值），大小写归一。
/// JF `MetadataField` 九项 + fbz 扩展 `Images`（锁图片：刷新/扫描/插件不再改 artwork，手动上传/删除不受限）。
pub const LOCKABLE_FIELDS: [&str; 10] = ["Cast", "Genres", "ProductionLocations", "Studios", "Tags", "Name", "Overview", "Runtime", "OfficialRating", "Images"];

pub fn normalize_locked_fields(values: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for v in values {
        if let Some(f) = LOCKABLE_FIELDS.iter().find(|f| f.eq_ignore_ascii_case(v.trim())) {
            if !out.iter().any(|x: &String| x == f) {
                out.push((*f).to_owned());
            }
        }
    }
    out
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct NameDto {
    pub name: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct PersonDto {
    pub name: String,
    pub role: Option<String>,
    #[serde(rename = "Type")]
    pub kind: Option<String>,
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

/// Jellyfin PersonKind → DB role_type allowlist。
pub fn person_role_type(kind: Option<&str>) -> &'static str {
    match kind.unwrap_or("").trim().to_ascii_lowercase().replace(['_', ' ', '-'], "").as_str() {
        "director" => "director",
        "writer" | "author" => "writer",
        "producer" => "producer",
        "composer" => "composer",
        "artist" | "albumartist" => "artist",
        "gueststar" => "guest_star",
        _ => "actor",
    }
}

pub fn to_patch(dto: UpdateItemDto) -> PatchPluginItemMetadataRequestDto {
    let clean = |list: Option<Vec<String>>| {
        list.map(|values| {
            let mut out: Vec<String> = Vec::new();
            for v in values.into_iter().map(|v| v.trim().to_owned()).filter(|v| !v.is_empty()) {
                if !out.iter().any(|x| x.eq_ignore_ascii_case(&v)) {
                    out.push(v);
                }
            }
            out
        })
    };
    PatchPluginItemMetadataRequestDto {
        title: non_empty(dto.name),
        original_title: non_empty(dto.original_title),
        sort_title: non_empty(dto.forced_sort_name).or_else(|| non_empty(dto.sort_name)),
        overview: non_empty(dto.overview),
        production_year: dto.production_year.filter(|y| *y > 0),
        premiere_date: non_empty(dto.premiere_date).map(|d| d.chars().take(10).collect()),
        official_rating: non_empty(dto.official_rating),
        community_rating: dto.community_rating,
        critic_rating: dto.critic_rating,
        runtime_ticks: None,
        external_ids: dto
            .provider_ids
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(provider, id)| {
                let id = non_empty(id)?;
                let provider = provider.trim().to_ascii_lowercase();
                (!provider.is_empty()).then_some(PatchPluginItemExternalIdDto { provider, external_id: id })
            })
            .collect(),
        genres: clean(dto.genres),
        studios: clean(dto.studios.map(|s| s.into_iter().map(|n| n.name).collect())),
        tags: clean(dto.tags),
        people: dto.people.map(|people| {
            people
                .into_iter()
                .filter(|p| !p.name.trim().is_empty())
                .enumerate()
                .map(|(i, p)| PatchPluginItemPersonDto {
                    name: p.name.trim().to_owned(),
                    role_type: person_role_type(p.kind.as_deref()).to_owned(),
                    role_name: non_empty(p.role),
                    sort_order: Some(i as i32),
                })
                .collect()
        }),
    }
}

pub async fn update_item(
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let user = authenticate_request_user(&state, &headers, &uri).await?;
    if !user.can_manage_server() {
        return Err(AppError::forbidden("server management permission required"));
    }
    let dto: UpdateItemDto = parse_emby_body(&headers, &body)?;
    let lock_data = dto.lock_data;
    let locked_fields = dto.locked_fields.as_deref().map(normalize_locked_fields);
    let Some(database) = state.database() else {
        return Err(AppError::internal("database is not configured"));
    };
    if !apply_item_metadata_patch(database.clone(), &item_id, to_patch(dto), lock_data, locked_fields).await? {
        return Err(AppError::not_found("media item not found"));
    }
    match crate::metadata::nfo_write::save_item_nfo(database, &item_id).await {
        Ok(Some(path)) => tracing::info!(item_id, path = %path.display(), "nfo saved"),
        Ok(None) => {}
        Err(err) => tracing::warn!(item_id, error = %err, "nfo save failed"),
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Jellyfin ProviderIds 键名：常见 provider 用 JF 规范大小写，其余首字母大写。
pub fn provider_key(provider: &str) -> String {
    match provider {
        "imdb" => "Imdb".to_owned(),
        "tmdb" => "Tmdb".to_owned(),
        "tvdb" => "Tvdb".to_owned(),
        "isbn" => "ISBN".to_owned(),
        "openlibrary" => "OpenLibrary".to_owned(),
        "googlebooks" => "GoogleBooks".to_owned(),
        "douban" => "Douban".to_owned(),
        other => {
            let mut chars = other.chars();
            chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
        }
    }
}

/// 条目详情补 JF 字段：`ProviderIds` / `Studios` / `Tags` / `SortName` / `ForcedSortName` / `LockData`
/// （元数据编辑器与客户端外链依赖；列表接口不加，避免 N+1）。
pub async fn attach_detail_fields(state: &AppState, mut value: serde_json::Value) -> serde_json::Value {
    let Some(pool) = state.database() else { return value };
    let Some(id) = value.get("Id").and_then(|v| v.as_str()).map(str::to_owned) else { return value };
    if id.len() < 32 || !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return value;
    }
    let Ok(Some(row)) = sqlx::query_as::<_, (i64, Option<String>, String, bool, Vec<String>)>(
        "select id, sort_title, title, lock_data, locked_fields from media_items where public_id = $1::uuid",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    else {
        return value;
    };
    let (internal, sort_title, title, lock_data, locked_fields) = row;
    let ids: Vec<(String, String)> = sqlx::query_as("select provider, external_id from media_external_ids where media_item_id = $1 order by provider")
        .bind(internal)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    let studios: Vec<(String, String)> = sqlx::query_as("select s.name, replace(s.public_id::text, '-', '') from media_item_studios x join studios s on s.id = x.studio_id where x.media_item_id = $1 order by s.name")
        .bind(internal)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    let tags: Vec<String> = sqlx::query_scalar("select t.name from media_item_tags x join tags t on t.id = x.tag_id where x.media_item_id = $1 order by t.name")
        .bind(internal)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    if let Some(obj) = value.as_object_mut() {
        let provider_ids: serde_json::Map<String, serde_json::Value> =
            ids.into_iter().map(|(p, v)| (provider_key(&p), serde_json::Value::String(v))).collect();
        obj.entry("ProviderIds").or_insert(serde_json::Value::Object(provider_ids));
        obj.entry("Studios").or_insert(serde_json::json!(studios
            .into_iter()
            .map(|(name, id)| serde_json::json!({"Name": name, "Id": id}))
            .collect::<Vec<_>>()));
        obj.entry("Tags").or_insert(serde_json::json!(tags));
        let sort = sort_title.filter(|s| !s.trim().is_empty());
        obj.entry("SortName").or_insert(serde_json::json!(sort.clone().unwrap_or(title)));
        if let Some(sort) = sort {
            obj.entry("ForcedSortName").or_insert(serde_json::json!(sort));
        }
        obj.insert("LockData".to_owned(), serde_json::json!(lock_data));
        obj.insert("LockedFields".to_owned(), serde_json::json!(locked_fields));
    }
    value
}

#[cfg(test)]
mod tests {
    #[test]
    fn locked_fields_normalize_to_jellyfin_enum() {
        let v = super::normalize_locked_fields(&["name".into(), "Cast".into(), "bogus".into(), "NAME".into()]);
        assert_eq!(v, ["Name", "Cast"]);
    }

    #[test]
    fn provider_keys_follow_jellyfin_casing() {
        assert_eq!(super::provider_key("imdb"), "Imdb");
        assert_eq!(super::provider_key("isbn"), "ISBN");
        assert_eq!(super::provider_key("musicbrainzalbum"), "Musicbrainzalbum");
    }

    use super::*;

    #[test]
    fn maps_jellyfin_base_item_dto_to_patch() {
        let dto: UpdateItemDto = serde_json::from_value(serde_json::json!({
            "Name": " New Title ", "Overview": "", "ProductionYear": 2001, "PremiereDate": "2001-05-04T00:00:00.0000000Z",
            "Genres": ["Drama", "drama", " "], "Studios": [{"Name": "A24"}],
            "People": [{"Name": "D", "Type": "Director"}, {"Name": "G", "Type": "GuestStar", "Role": "Cop"}, {"Name": "X", "Type": "Lyricist"}],
            "ProviderIds": {"Imdb": "tt0000001", "Tmdb": "", "Tvdb": null},
            "LockData": true, "Id": "ignored"
        })).unwrap();
        let p = to_patch(dto);
        assert_eq!(p.title.as_deref(), Some("New Title"));
        assert_eq!(p.overview, None, "empty overview is not written");
        assert_eq!(p.premiere_date.as_deref(), Some("2001-05-04"));
        assert_eq!(p.genres, Some(vec!["Drama".to_owned()]));
        assert_eq!(p.studios, Some(vec!["A24".to_owned()]));
        let people = p.people.unwrap();
        assert_eq!(people.iter().map(|x| x.role_type.as_str()).collect::<Vec<_>>(), ["director", "guest_star", "actor"]);
        assert_eq!(people[1].role_name.as_deref(), Some("Cop"));
        assert_eq!(p.external_ids.len(), 1);
        assert_eq!(p.external_ids[0].provider, "imdb");
    }
}
