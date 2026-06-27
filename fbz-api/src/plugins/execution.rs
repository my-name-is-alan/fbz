use std::{
    error::Error,
    fmt::{Display, Formatter},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use http::{Uri, uri::InvalidUri};
use reqwest::{
    Client,
    header::{CONTENT_TYPE, USER_AGENT},
};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use tracing::warn;

use crate::{
    config::PluginConfig,
    db::DbPool,
    plugins::{
        hooks::PLUGIN_HOOK_DISPATCH_EVENT,
        host::{IssuedPluginHostToken, PluginHostRepository},
        wasi::{PluginWasiExecution, PluginWasiRuntime},
    },
};

const PLUGIN_EXECUTOR_ID: &str = "fbz-api-plugin-executor";
const PLUGIN_SIGNATURE_VERSION: &str = "v1";
const HMAC_SHA256_BLOCK_SIZE: usize = 64;
const MAX_RESPONSE_BODY_BYTES: usize = 4096;
const MAX_ERROR_BYTES: usize = 2048;
const MIN_DISPATCH_LEASE_SECONDS: u64 = 300;
const DISPATCH_LEASE_GRACE_SECONDS: u64 = 60;
const STALE_EXECUTION_MESSAGE: &str = "plugin execution lease expired; dispatch will retry";
const CLAIM_NEXT_PLUGIN_DISPATCH_SQL: &str = r#"
            with claimed as (
                select id, status as prior_status, locked_by as prior_locked_by
                from event_outbox
                where event_type = $1
                  and available_at <= now()
                  and attempts < max_attempts
                  and (
                      status in ('pending', 'failed')
                      or (status = 'delivering' and locked_until <= now())
                  )
                order by available_at asc, id asc
                for update skip locked
                limit 1
            )
            update event_outbox
            set status = 'delivering',
                attempts = attempts + 1,
                locked_by = $2,
                locked_until = now() + ($3::bigint * interval '1 second'),
                last_error = null
            from claimed
            where event_outbox.id = claimed.id
            returning
                event_outbox.id as id,
                event_outbox.public_id::text as public_id,
                event_outbox.payload as payload,
                event_outbox.attempts as attempts,
                event_outbox.max_attempts as max_attempts,
                claimed.prior_status as prior_status,
                claimed.prior_locked_by as prior_locked_by
            "#;
const EXPIRE_STALE_PLUGIN_EXECUTIONS_SQL: &str = r#"
with stale_execution_candidates as (
    select run.id
    from plugin_execution_runs run
    join event_outbox outbox on outbox.id = run.outbox_event_id
    where run.status = 'running'
      and run.finished_at is null
      and (
          (outbox.status = 'delivering' and outbox.locked_until <= now())
          or outbox.status in ('delivered', 'failed', 'discarded')
      )
    order by run.started_at asc, run.id asc
    limit 1000
    for update of run skip locked
),
expired_runs as (
    update plugin_execution_runs run
    set status = 'failed',
        error_message = coalesce(run.error_message, $1),
        finished_at = coalesce(run.finished_at, now()),
        duration_ms = coalesce(
            run.duration_ms,
            least(
                2147483647,
                greatest(
                    0,
                    floor(extract(epoch from (now() - run.started_at)) * 1000)
                )
            )::integer
        )
    from stale_execution_candidates candidates
    where run.id = candidates.id
    returning run.id
),
revoked_tokens as (
    update plugin_host_tokens token
    set revoked_at = coalesce(token.revoked_at, now())
    from expired_runs
    where token.execution_run_id = expired_runs.id
      and token.revoked_at is null
    returning token.id
)
select
    (select count(*) from expired_runs) as expired_runs,
    (select count(*) from revoked_tokens) as revoked_tokens
"#;

#[derive(Clone)]
pub struct PluginExecutionService {
    pool: DbPool,
    config: PluginConfig,
    host_base_url: String,
    client: PluginExecutionClient,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginExecutionSummary {
    pub outbox_event_id: String,
    pub plugin_id: String,
    pub handler: String,
    pub outbox_status: String,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginStaleExecutionRecoverySummary {
    pub expired_runs: i64,
    pub revoked_tokens: i64,
}

impl PluginStaleExecutionRecoverySummary {
    pub fn recovered_anything(&self) -> bool {
        self.expired_runs > 0 || self.revoked_tokens > 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ClaimedPluginDispatch {
    id: i64,
    public_id: String,
    payload: Value,
    attempt: i32,
    max_attempts: i32,
    recovered_stale_lease: bool,
    prior_locked_by: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct PluginDispatchPayload {
    plugin_id: String,
    package_id: String,
    hook_id: Option<i64>,
    handler: String,
    hook_event: String,
    source: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PluginExecutionTarget {
    plugin_id: String,
    package_version: String,
    runtime: String,
    entrypoint: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PluginRuntimeOutput {
    response_status: Option<i32>,
    response_body: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginExecutionError {
    Database(String),
}

#[derive(Debug)]
enum PluginRuntimeError {
    InvalidUri(String),
    UnsupportedScheme(String),
    MissingHost,
    HostNotAllowed(String),
    Request(String),
    Timeout,
    ResponseTooLarge { limit: usize },
    HttpStatus { status: i32, body: String },
    Wasi(String),
    UnsupportedRuntime(String),
}

impl PluginExecutionService {
    pub fn new(pool: DbPool, config: PluginConfig, host_base_url: String) -> Self {
        Self {
            pool,
            config,
            host_base_url,
            client: PluginExecutionClient::new(),
        }
    }

    pub async fn run_next_dispatch(
        &self,
    ) -> Result<Option<PluginExecutionSummary>, PluginExecutionError> {
        let Some(event) = self.claim_next_dispatch().await? else {
            return Ok(None);
        };

        if event.recovered_stale_lease {
            warn!(
                outbox_event_id = %event.public_id,
                prior_locked_by = event.prior_locked_by.as_deref().unwrap_or("unknown"),
                attempt = event.attempt,
                max_attempts = event.max_attempts,
                "recovered stale plugin dispatch lease"
            );
        }

        let dispatch = match parse_dispatch_payload(&event.payload) {
            Ok(dispatch) => dispatch,
            Err(err) => {
                let message = truncate_error(&err);
                let outbox_status = self.mark_event_failure(&event, &message).await?;
                return Ok(Some(PluginExecutionSummary {
                    outbox_event_id: event.public_id,
                    plugin_id: String::new(),
                    handler: String::new(),
                    outbox_status,
                    error_message: Some(message),
                }));
            }
        };

        let target = match self.resolve_target(&dispatch).await? {
            Some(target) => target,
            None => {
                let message = format!(
                    "plugin `{}` package `{}` is not approved, enabled, or active",
                    dispatch.plugin_id, dispatch.package_id
                );
                let outbox_status = self.mark_event_failure(&event, &message).await?;
                return Ok(Some(PluginExecutionSummary {
                    outbox_event_id: event.public_id,
                    plugin_id: dispatch.plugin_id,
                    handler: dispatch.handler,
                    outbox_status,
                    error_message: Some(message),
                }));
            }
        };

        let run_id = self
            .create_execution_run(&event, &dispatch, &target)
            .await?;
        let host_token = self
            .issue_host_token_for_target(&dispatch, &target, run_id)
            .await?;
        let started = Instant::now();
        let result = self
            .client
            .execute(
                &target,
                &event.payload,
                Duration::from_millis(self.config.timeout_ms),
                PluginRuntimeExecutionContext {
                    plugin_id: &dispatch.plugin_id,
                    handler: &dispatch.handler,
                    idempotency_key: &event.public_id,
                    host_base_url: &self.host_base_url,
                    host_token: host_token.as_ref().map(|token| token.token.as_str()),
                    config: &self.config,
                },
            )
            .await;
        self.revoke_host_token(host_token.as_ref()).await;

        match result {
            Ok(output) => {
                self.finish_execution_run_success(run_id, started.elapsed(), &output)
                    .await?;
                self.mark_event_delivered(event.id).await?;
                Ok(Some(PluginExecutionSummary {
                    outbox_event_id: event.public_id,
                    plugin_id: dispatch.plugin_id,
                    handler: dispatch.handler,
                    outbox_status: "delivered".to_owned(),
                    error_message: None,
                }))
            }
            Err(err) => {
                let message = truncate_error(&err.to_string());
                self.finish_execution_run_failure(run_id, started.elapsed(), &err, &message)
                    .await?;
                let outbox_status = self.mark_event_failure(&event, &message).await?;
                Ok(Some(PluginExecutionSummary {
                    outbox_event_id: event.public_id,
                    plugin_id: dispatch.plugin_id,
                    handler: dispatch.handler,
                    outbox_status,
                    error_message: Some(message),
                }))
            }
        }
    }

    pub async fn recover_stale_execution_runs(
        &self,
    ) -> Result<PluginStaleExecutionRecoverySummary, PluginExecutionError> {
        let row = sqlx::query(EXPIRE_STALE_PLUGIN_EXECUTIONS_SQL)
            .bind(STALE_EXECUTION_MESSAGE)
            .fetch_one(&self.pool)
            .await
            .map_err(PluginExecutionError::from)?;

        Ok(PluginStaleExecutionRecoverySummary {
            expired_runs: row.try_get("expired_runs")?,
            revoked_tokens: row.try_get("revoked_tokens")?,
        })
    }

    async fn claim_next_dispatch(
        &self,
    ) -> Result<Option<ClaimedPluginDispatch>, PluginExecutionError> {
        let row = sqlx::query(CLAIM_NEXT_PLUGIN_DISPATCH_SQL)
            .bind(PLUGIN_HOOK_DISPATCH_EVENT)
            .bind(PLUGIN_EXECUTOR_ID)
            .bind(dispatch_lease_seconds(self.config.timeout_ms) as i64)
            .fetch_optional(&self.pool)
            .await
            .map_err(PluginExecutionError::from)?;

        row.map(|row| -> Result<ClaimedPluginDispatch, sqlx::Error> {
            let prior_status: String = row.try_get("prior_status")?;
            Ok(ClaimedPluginDispatch {
                id: row.try_get("id")?,
                public_id: row.try_get("public_id")?,
                payload: row.try_get("payload")?,
                attempt: row.try_get("attempts")?,
                max_attempts: row.try_get("max_attempts")?,
                recovered_stale_lease: prior_status == "delivering",
                prior_locked_by: row.try_get("prior_locked_by")?,
            })
        })
        .transpose()
        .map_err(PluginExecutionError::from)
    }

    async fn resolve_target(
        &self,
        dispatch: &PluginDispatchPayload,
    ) -> Result<Option<PluginExecutionTarget>, PluginExecutionError> {
        let row = sqlx::query(
            r#"
            select
                pkg.plugin_id,
                pkg.package_version,
                pkg.runtime,
                pkg.entrypoint
            from plugin_installations pi
            join plugin_packages pkg on pkg.id = pi.active_package_id
            where pi.plugin_id = $1
              and pkg.public_id = case
                  when $2::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                  then $2::uuid
                  else null::uuid
              end
              and pi.enabled = true
              and pi.approval_status = 'approved'
              and pkg.package_status = 'approved'
            "#,
        )
        .bind(dispatch.plugin_id.trim())
        .bind(dispatch.package_id.trim())
        .fetch_optional(&self.pool)
        .await
        .map_err(PluginExecutionError::from)?;

        row.map(|row| -> Result<PluginExecutionTarget, sqlx::Error> {
            Ok(PluginExecutionTarget {
                plugin_id: row.try_get("plugin_id")?,
                package_version: row.try_get("package_version")?,
                runtime: row.try_get("runtime")?,
                entrypoint: row.try_get("entrypoint")?,
            })
        })
        .transpose()
        .map_err(PluginExecutionError::from)
    }

    async fn create_execution_run(
        &self,
        event: &ClaimedPluginDispatch,
        dispatch: &PluginDispatchPayload,
        target: &PluginExecutionTarget,
    ) -> Result<i64, PluginExecutionError> {
        let row = sqlx::query(
            r#"
            insert into plugin_execution_runs (
                outbox_event_id,
                outbox_event_public_id,
                attempt,
                plugin_id,
                package_id,
                hook_id,
                handler,
                event_key,
                runtime,
                entrypoint,
                request_payload
            )
            values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            returning id
            "#,
        )
        .bind(event.id)
        .bind(&event.public_id)
        .bind(event.attempt)
        .bind(dispatch.plugin_id.trim())
        .bind(dispatch.package_id.trim())
        .bind(dispatch.hook_id)
        .bind(dispatch.handler.trim())
        .bind(dispatch.hook_event.trim())
        .bind(target.runtime.trim())
        .bind(target.entrypoint.trim())
        .bind(&event.payload)
        .fetch_one(&self.pool)
        .await
        .map_err(PluginExecutionError::from)?;

        row.try_get("id").map_err(PluginExecutionError::from)
    }

    async fn issue_host_token_for_target(
        &self,
        dispatch: &PluginDispatchPayload,
        target: &PluginExecutionTarget,
        execution_run_id: i64,
    ) -> Result<Option<IssuedPluginHostToken>, PluginExecutionError> {
        if target.runtime != "http" && target.runtime != "wasi" {
            return Ok(None);
        }

        PluginHostRepository::new(self.pool.clone())
            .issue_execution_token(&dispatch.plugin_id, &dispatch.package_id, execution_run_id)
            .await
            .map(Some)
            .map_err(PluginExecutionError::from)
    }

    async fn revoke_host_token(&self, host_token: Option<&IssuedPluginHostToken>) {
        let Some(host_token) = host_token else {
            return;
        };

        if let Err(err) = PluginHostRepository::new(self.pool.clone())
            .revoke_token(host_token.id)
            .await
        {
            warn!(
                error = %err,
                token_prefix = %host_token.prefix,
                "failed to revoke plugin host token"
            );
        }
    }

    async fn finish_execution_run_success(
        &self,
        run_id: i64,
        elapsed: Duration,
        output: &PluginRuntimeOutput,
    ) -> Result<(), PluginExecutionError> {
        sqlx::query(
            r#"
            update plugin_execution_runs
            set status = 'succeeded',
                response_status = $2,
                response_body = $3,
                finished_at = now(),
                duration_ms = $4
            where id = $1
            "#,
        )
        .bind(run_id)
        .bind(output.response_status)
        .bind(output.response_body.as_deref())
        .bind(duration_millis_i32(elapsed))
        .execute(&self.pool)
        .await
        .map_err(PluginExecutionError::from)?;

        Ok(())
    }

    async fn finish_execution_run_failure(
        &self,
        run_id: i64,
        elapsed: Duration,
        err: &PluginRuntimeError,
        message: &str,
    ) -> Result<(), PluginExecutionError> {
        sqlx::query(
            r#"
            update plugin_execution_runs
            set status = 'failed',
                response_status = $2,
                response_body = $3,
                error_message = $4,
                finished_at = now(),
                duration_ms = $5
            where id = $1
            "#,
        )
        .bind(run_id)
        .bind(err.response_status())
        .bind(err.response_body())
        .bind(message)
        .bind(duration_millis_i32(elapsed))
        .execute(&self.pool)
        .await
        .map_err(PluginExecutionError::from)?;

        Ok(())
    }

    async fn mark_event_delivered(&self, event_id: i64) -> Result<(), PluginExecutionError> {
        sqlx::query(
            r#"
            update event_outbox
            set status = 'delivered',
                locked_by = null,
                locked_until = null,
                last_error = null,
                delivered_at = now()
            where id = $1
            "#,
        )
        .bind(event_id)
        .execute(&self.pool)
        .await
        .map_err(PluginExecutionError::from)?;

        Ok(())
    }

    async fn mark_event_failure(
        &self,
        event: &ClaimedPluginDispatch,
        message: &str,
    ) -> Result<String, PluginExecutionError> {
        let outbox_status = if event.attempt >= event.max_attempts {
            "discarded"
        } else {
            "failed"
        };
        let retry_delay_seconds = if outbox_status == "failed" {
            retry_delay_seconds(event.attempt)
        } else {
            0
        };

        sqlx::query(
            r#"
            update event_outbox
            set status = $2,
                locked_by = null,
                locked_until = null,
                last_error = $3,
                available_at = case
                    when $2 = 'failed' then now() + ($4::bigint * interval '1 second')
                    else available_at
                end
            where id = $1
            "#,
        )
        .bind(event.id)
        .bind(outbox_status)
        .bind(message)
        .bind(retry_delay_seconds)
        .execute(&self.pool)
        .await
        .map_err(PluginExecutionError::from)?;

        if outbox_status == "failed" {
            warn!(
                event_id = event.id,
                outbox_event_id = %event.public_id,
                attempt = event.attempt,
                max_attempts = event.max_attempts,
                retry_delay_seconds,
                error = %message,
                "plugin dispatch failed; scheduled retry"
            );
        }

        Ok(outbox_status.to_owned())
    }
}

#[derive(Clone)]
struct PluginExecutionClient {
    client: Client,
    wasi: PluginWasiRuntime,
}

#[derive(Clone, Copy)]
struct PluginRuntimeExecutionContext<'a> {
    plugin_id: &'a str,
    handler: &'a str,
    idempotency_key: &'a str,
    host_base_url: &'a str,
    host_token: Option<&'a str>,
    config: &'a PluginConfig,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PluginHttpSignatureHeaders {
    version: String,
    timestamp: String,
    body_sha256: String,
    signature: String,
}

impl PluginExecutionClient {
    fn new() -> Self {
        Self {
            client: Client::new(),
            wasi: PluginWasiRuntime::new(),
        }
    }

    async fn execute(
        &self,
        target: &PluginExecutionTarget,
        payload: &Value,
        timeout_duration: Duration,
        context: PluginRuntimeExecutionContext<'_>,
    ) -> Result<PluginRuntimeOutput, PluginRuntimeError> {
        match target.runtime.trim() {
            "http" => {
                self.execute_http(&target.entrypoint, payload, timeout_duration, context)
                    .await
            }
            "wasi" => {
                self.execute_wasi(target, payload, timeout_duration, context)
                    .await
            }
            other => Err(PluginRuntimeError::UnsupportedRuntime(other.to_owned())),
        }
    }

    async fn execute_http(
        &self,
        entrypoint: &str,
        payload: &Value,
        timeout_duration: Duration,
        context: PluginRuntimeExecutionContext<'_>,
    ) -> Result<PluginRuntimeOutput, PluginRuntimeError> {
        let uri = validate_http_entrypoint(entrypoint)?;
        ensure_http_host_allowed(&uri, &context.config.http_allowed_hosts)?;

        let body = serde_json::to_vec(payload)
            .map_err(|err| PluginRuntimeError::Request(err.to_string()))?;
        let mut request = self
            .client
            .post(entrypoint.trim())
            .timeout(timeout_duration)
            .header(CONTENT_TYPE, "application/json")
            .header(USER_AGENT, "fbz-api-plugin-executor/0.1")
            .header("x-fbz-plugin-id", context.plugin_id)
            .header("x-fbz-plugin-idempotency-key", context.idempotency_key)
            .header("x-fbz-host-base-url", context.host_base_url);
        if let Some(host_token) = context.host_token {
            request = request.header("x-fbz-plugin-token", host_token);
        }
        if let Some(secret) = context
            .config
            .secret_key
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            let signature = plugin_http_signature_headers(
                secret,
                context.plugin_id,
                context.idempotency_key,
                unix_timestamp_seconds(),
                &body,
            );
            request = request
                .header("x-fbz-plugin-signature-version", signature.version)
                .header("x-fbz-plugin-signature-timestamp", signature.timestamp)
                .header("x-fbz-plugin-body-sha256", signature.body_sha256)
                .header("x-fbz-plugin-signature", signature.signature);
        }

        let response = request
            .body(body)
            .send()
            .await
            .map_err(reqwest_runtime_error)?;
        let status = response.status();
        let response_body =
            read_limited_response_body(response, context.config.http_max_response_body_bytes)
                .await?;
        let response_text = truncate_body(&response_body);

        if !status.is_success() {
            return Err(PluginRuntimeError::HttpStatus {
                status: status.as_u16() as i32,
                body: response_text,
            });
        }

        Ok(PluginRuntimeOutput {
            response_status: Some(status.as_u16() as i32),
            response_body: Some(response_text),
        })
    }

    async fn execute_wasi(
        &self,
        target: &PluginExecutionTarget,
        payload: &Value,
        timeout_duration: Duration,
        context: PluginRuntimeExecutionContext<'_>,
    ) -> Result<PluginRuntimeOutput, PluginRuntimeError> {
        let output = self
            .wasi
            .execute(PluginWasiExecution {
                package_dir: context.config.package_dir.clone(),
                data_dir: context.config.data_dir.clone(),
                cache_dir: context.config.cache_dir.clone(),
                tmp_dir: context.config.tmp_dir.clone(),
                plugin_id: target.plugin_id.trim().to_owned(),
                package_version: target.package_version.trim().to_owned(),
                entrypoint: target.entrypoint.trim().to_owned(),
                handler: context.handler.trim().to_owned(),
                idempotency_key: context.idempotency_key.trim().to_owned(),
                host_base_url: context.host_base_url.trim().to_owned(),
                host_token: context.host_token.map(str::to_owned),
                payload: payload.clone(),
                timeout: timeout_duration,
                memory_limit_mb: context.config.memory_limit_mb,
                fuel: context.config.wasi_fuel,
                stdio_max_bytes: context.config.wasi_stdio_max_bytes,
                max_module_bytes: context.config.wasi_max_module_bytes,
                tmp_max_age: Duration::from_secs(context.config.tmp_max_age_seconds),
            })
            .await
            .map_err(|err| match err {
                crate::plugins::wasi::PluginWasiError::Timeout => PluginRuntimeError::Timeout,
                other => PluginRuntimeError::Wasi(other.to_string()),
            })?;

        Ok(PluginRuntimeOutput {
            response_status: None,
            response_body: output.response_body,
        })
    }
}

fn validate_http_entrypoint(entrypoint: &str) -> Result<Uri, PluginRuntimeError> {
    let uri = entrypoint
        .trim()
        .parse::<Uri>()
        .map_err(|err| PluginRuntimeError::InvalidUri(err.to_string()))?;
    match uri.scheme_str() {
        Some("http" | "https") => {}
        Some(other) => return Err(PluginRuntimeError::UnsupportedScheme(other.to_owned())),
        None => return Err(PluginRuntimeError::UnsupportedScheme(String::new())),
    }
    uri.host().ok_or(PluginRuntimeError::MissingHost)?;
    Ok(uri)
}

fn ensure_http_host_allowed(uri: &Uri, allowed_hosts: &[String]) -> Result<(), PluginRuntimeError> {
    let host = uri.host().ok_or(PluginRuntimeError::MissingHost)?;
    if http_host_allowed(host, allowed_hosts) {
        return Ok(());
    }

    Err(PluginRuntimeError::HostNotAllowed(host.to_owned()))
}

fn http_host_allowed(host: &str, allowed_hosts: &[String]) -> bool {
    let host = normalize_http_host(host);
    if host.is_empty() {
        return false;
    }

    allowed_hosts.iter().any(|allowed| {
        let allowed = normalize_http_host(allowed);
        if let Some(suffix) = allowed.strip_prefix("*.") {
            return host.len() > suffix.len()
                && host.ends_with(suffix)
                && host.as_bytes()[host.len() - suffix.len() - 1] == b'.';
        }
        host == allowed
    })
}

fn normalize_http_host(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

fn plugin_http_signature_headers(
    secret: &str,
    plugin_id: &str,
    idempotency_key: &str,
    timestamp: u64,
    body: &[u8],
) -> PluginHttpSignatureHeaders {
    let body_sha256 = sha256_hex(body);
    let canonical = format!(
        "{}\n{}\n{}\n{}\n{}",
        PLUGIN_SIGNATURE_VERSION, timestamp, plugin_id, idempotency_key, body_sha256
    );
    let signature = hmac_sha256_hex(secret.as_bytes(), canonical.as_bytes());

    PluginHttpSignatureHeaders {
        version: PLUGIN_SIGNATURE_VERSION.to_owned(),
        timestamp: timestamp.to_string(),
        body_sha256,
        signature: format!("sha256={signature}"),
    }
}

fn hmac_sha256_hex(secret: &[u8], message: &[u8]) -> String {
    let mut key = [0_u8; HMAC_SHA256_BLOCK_SIZE];
    if secret.len() > HMAC_SHA256_BLOCK_SIZE {
        let digest = Sha256::digest(secret);
        key[..digest.len()].copy_from_slice(&digest);
    } else {
        key[..secret.len()].copy_from_slice(secret);
    }

    let mut outer_pad = [0x5c_u8; HMAC_SHA256_BLOCK_SIZE];
    let mut inner_pad = [0x36_u8; HMAC_SHA256_BLOCK_SIZE];
    for index in 0..HMAC_SHA256_BLOCK_SIZE {
        outer_pad[index] ^= key[index];
        inner_pad[index] ^= key[index];
    }

    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_hash);
    hex_lower(&outer.finalize())
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn unix_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn reqwest_runtime_error(error: reqwest::Error) -> PluginRuntimeError {
    if error.is_timeout() {
        return PluginRuntimeError::Timeout;
    }

    PluginRuntimeError::Request(error.to_string())
}

async fn read_limited_response_body(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Bytes, PluginRuntimeError> {
    if response
        .content_length()
        .is_some_and(|len| len > limit as u64)
    {
        return Err(PluginRuntimeError::ResponseTooLarge { limit });
    }

    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(reqwest_runtime_error)? {
        append_limited_response_chunk(&mut body, &chunk, limit)?;
    }

    Ok(Bytes::from(body))
}

fn append_limited_response_chunk(
    body: &mut Vec<u8>,
    chunk: &Bytes,
    limit: usize,
) -> Result<(), PluginRuntimeError> {
    if body.len().saturating_add(chunk.len()) > limit {
        return Err(PluginRuntimeError::ResponseTooLarge { limit });
    }
    body.extend_from_slice(chunk);
    Ok(())
}

fn parse_dispatch_payload(payload: &Value) -> Result<PluginDispatchPayload, String> {
    let dispatch: PluginDispatchPayload =
        serde_json::from_value(payload.clone()).map_err(|err| err.to_string())?;
    validate_non_empty("pluginId", &dispatch.plugin_id)?;
    validate_non_empty("packageId", &dispatch.package_id)?;
    validate_non_empty("handler", &dispatch.handler)?;
    validate_non_empty("hookEvent", &dispatch.hook_event)?;
    Ok(dispatch)
}

fn validate_non_empty(field: &'static str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} is required"));
    }
    Ok(())
}

fn retry_delay_seconds(attempt: i32) -> i64 {
    let bounded_attempt = attempt.clamp(1, 6) as u32;
    5_i64 * 2_i64.pow(bounded_attempt - 1)
}

fn dispatch_lease_seconds(timeout_ms: u64) -> u64 {
    let timeout_seconds = timeout_ms.saturating_add(999) / 1000;
    MIN_DISPATCH_LEASE_SECONDS.max(timeout_seconds.saturating_add(DISPATCH_LEASE_GRACE_SECONDS))
}

fn duration_millis_i32(duration: Duration) -> i32 {
    duration.as_millis().min(i32::MAX as u128) as i32
}

fn truncate_error(message: &str) -> String {
    truncate_str(message, MAX_ERROR_BYTES)
}

fn truncate_body(bytes: &Bytes) -> String {
    let len = bytes.len().min(MAX_RESPONSE_BODY_BYTES);
    String::from_utf8_lossy(&bytes[..len]).to_string()
}

fn truncate_str(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }

    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

impl PluginRuntimeError {
    fn response_status(&self) -> Option<i32> {
        match self {
            Self::HttpStatus { status, .. } => Some(*status),
            _ => None,
        }
    }

    fn response_body(&self) -> Option<&str> {
        match self {
            Self::HttpStatus { body, .. } => Some(body),
            _ => None,
        }
    }
}

impl Display for PluginRuntimeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidUri(err) => write!(f, "invalid plugin HTTP entrypoint: {err}"),
            Self::UnsupportedScheme(scheme) if scheme.is_empty() => {
                f.write_str("plugin HTTP entrypoint is missing a scheme")
            }
            Self::UnsupportedScheme(scheme) => {
                write!(f, "unsupported plugin HTTP scheme `{scheme}`")
            }
            Self::MissingHost => f.write_str("plugin HTTP entrypoint is missing a host"),
            Self::HostNotAllowed(host) => {
                write!(f, "plugin HTTP host `{host}` is not allowed")
            }
            Self::Request(err) => write!(f, "plugin HTTP request failed: {err}"),
            Self::Timeout => f.write_str("plugin execution timed out"),
            Self::ResponseTooLarge { limit } => {
                write!(f, "plugin HTTP response body exceeded {limit} bytes")
            }
            Self::HttpStatus { status, body } => {
                write!(f, "plugin HTTP endpoint returned {status}: {body}")
            }
            Self::Wasi(message) => write!(f, "plugin WASI execution failed: {message}"),
            Self::UnsupportedRuntime(runtime) => {
                write!(f, "unsupported plugin runtime: {runtime}")
            }
        }
    }
}

impl Error for PluginRuntimeError {}

impl From<sqlx::Error> for PluginExecutionError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error.to_string())
    }
}

impl Display for PluginExecutionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(err) => write!(f, "database error: {err}"),
        }
    }
}

impl Error for PluginExecutionError {}

impl From<InvalidUri> for PluginRuntimeError {
    fn from(error: InvalidUri) -> Self {
        Self::InvalidUri(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use http::StatusCode;
    use serde_json::json;

    use super::*;

    #[test]
    fn parse_dispatch_payload_requires_core_fields() {
        let payload = json!({
            "pluginId": "dev.fbz.notify",
            "packageId": "package-1",
            "hookId": 3,
            "handler": "hooks.onScanCompleted",
            "hookEvent": "library.scan.completed",
            "source": {"payload": {"scannedFiles": 2}}
        });

        let dispatch = parse_dispatch_payload(&payload).unwrap();

        assert_eq!(dispatch.plugin_id, "dev.fbz.notify");
        assert_eq!(dispatch.hook_id, Some(3));
        assert_eq!(dispatch.hook_event, "library.scan.completed");
        assert_eq!(dispatch.source["payload"]["scannedFiles"], 2);
    }

    #[test]
    fn parse_dispatch_payload_accepts_scheduled_dispatch_without_hook_id() {
        let payload = json!({
            "pluginId": "dev.fbz.notify",
            "packageId": "package-1",
            "hookId": null,
            "handler": "schedules.sync",
            "hookEvent": "scheduler.tick",
            "source": {
                "aggregateType": "plugin_schedule",
                "aggregateId": "dev.fbz.notify.sync",
                "payload": {"taskKey": "dev.fbz.notify.sync"}
            }
        });

        let dispatch = parse_dispatch_payload(&payload).unwrap();

        assert_eq!(dispatch.hook_id, None);
        assert_eq!(dispatch.handler, "schedules.sync");
        assert_eq!(dispatch.hook_event, "scheduler.tick");
    }

    #[test]
    fn parse_dispatch_payload_rejects_empty_handler() {
        let payload = json!({
            "pluginId": "dev.fbz.notify",
            "packageId": "package-1",
            "hookId": 3,
            "handler": "",
            "hookEvent": "library.scan.completed",
            "source": {}
        });

        let err = parse_dispatch_payload(&payload).unwrap_err();

        assert!(err.contains("handler"));
    }

    #[test]
    fn retry_delay_is_bounded_exponential_backoff() {
        assert_eq!(retry_delay_seconds(1), 5);
        assert_eq!(retry_delay_seconds(2), 10);
        assert_eq!(retry_delay_seconds(6), 160);
        assert_eq!(retry_delay_seconds(12), 160);
    }

    #[test]
    fn plugin_dispatch_retry_logs_structured_event_context() {
        let production_source = include_str!("execution.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("execution source should include production section");

        assert!(production_source.contains("outbox_event_id = %event.public_id"));
        assert!(production_source.contains("attempt = event.attempt"));
        assert!(production_source.contains("max_attempts = event.max_attempts"));
        assert!(production_source.contains("retry_delay_seconds"));
        assert!(production_source.contains("plugin dispatch failed; scheduled retry"));
    }

    #[test]
    fn plugin_dispatch_stale_lease_reclaim_logs_prior_state() {
        let production_source = include_str!("execution.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("execution source should include production section");

        assert!(CLAIM_NEXT_PLUGIN_DISPATCH_SQL.contains("with claimed as ("));
        assert!(CLAIM_NEXT_PLUGIN_DISPATCH_SQL.contains("status as prior_status"));
        assert!(CLAIM_NEXT_PLUGIN_DISPATCH_SQL.contains("locked_by as prior_locked_by"));
        assert!(
            CLAIM_NEXT_PLUGIN_DISPATCH_SQL.contains("claimed.prior_locked_by as prior_locked_by")
        );
        assert!(
            production_source.contains("recovered_stale_lease: prior_status == \"delivering\"")
        );
        assert!(production_source.contains("\"recovered stale plugin dispatch lease\""));
        assert!(production_source.contains("outbox_event_id = %event.public_id"));
        assert!(production_source.contains("prior_locked_by = event.prior_locked_by"));
        assert!(production_source.contains("attempt = event.attempt"));
        assert!(production_source.contains("max_attempts = event.max_attempts"));
    }

    #[test]
    fn dispatch_lease_uses_minimum_and_timeout_cushion() {
        assert_eq!(dispatch_lease_seconds(5_000), 300);
        assert_eq!(dispatch_lease_seconds(300_001), 361);
        assert_eq!(dispatch_lease_seconds(900_000), 960);
    }

    #[test]
    fn stale_execution_recovery_sql_closes_runs_and_tokens() {
        let stale_recovery_migration =
            include_str!("../../migrations/0074_plugin_execution_stale_recovery_index.sql");
        let normalized = EXPIRE_STALE_PLUGIN_EXECUTIONS_SQL
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        assert!(normalized.contains("with stale_execution_candidates as"));
        assert!(normalized.contains("from plugin_execution_runs run"));
        assert!(normalized.contains("join event_outbox outbox"));
        assert!(normalized.contains("run.status = 'running'"));
        assert!(normalized.contains("run.finished_at is null"));
        assert!(normalized.contains("order by run.started_at asc, run.id asc"));
        assert!(normalized.contains("limit 1000"));
        assert!(normalized.contains("for update of run skip locked"));
        assert!(normalized.contains("from stale_execution_candidates candidates"));
        assert!(normalized.contains("run.id = candidates.id"));
        assert!(
            !normalized.contains("with expired_runs as ( update plugin_execution_runs run"),
            "stale plugin execution recovery should not update every matching running run directly"
        );
        assert!(stale_recovery_migration.contains("idx_plugin_execution_runs_stale_recovery"));
        assert!(
            stale_recovery_migration.contains("on plugin_execution_runs (started_at asc, id asc)")
        );
        assert!(stale_recovery_migration.contains("where status = 'running'"));
        assert!(stale_recovery_migration.contains("finished_at is null"));

        assert!(EXPIRE_STALE_PLUGIN_EXECUTIONS_SQL.contains("plugin_execution_runs run"));
        assert!(EXPIRE_STALE_PLUGIN_EXECUTIONS_SQL.contains("plugin_host_tokens token"));
        assert!(EXPIRE_STALE_PLUGIN_EXECUTIONS_SQL.contains("run.status = 'running'"));
        assert!(
            EXPIRE_STALE_PLUGIN_EXECUTIONS_SQL
                .contains("outbox.status = 'delivering' and outbox.locked_until <= now()")
        );
        assert!(
            EXPIRE_STALE_PLUGIN_EXECUTIONS_SQL
                .contains("outbox.status in ('delivered', 'failed', 'discarded')")
        );
        assert!(EXPIRE_STALE_PLUGIN_EXECUTIONS_SQL.contains("token.revoked_at is null"));
    }

    #[test]
    fn plugin_dispatch_claim_query_matches_scale_indexes() {
        let migration = include_str!("../../migrations/0044_plugin_dispatch_claim_indexes.sql");

        assert!(CLAIM_NEXT_PLUGIN_DISPATCH_SQL.contains("where event_type = $1"));
        assert!(CLAIM_NEXT_PLUGIN_DISPATCH_SQL.contains("status in ('pending', 'failed')"));
        assert!(
            CLAIM_NEXT_PLUGIN_DISPATCH_SQL
                .contains("status = 'delivering' and locked_until <= now()")
        );
        assert!(CLAIM_NEXT_PLUGIN_DISPATCH_SQL.contains("attempts < max_attempts"));
        assert!(CLAIM_NEXT_PLUGIN_DISPATCH_SQL.contains("for update skip locked"));
        assert!(migration.contains("idx_event_outbox_plugin_dispatch_available"));
        assert!(migration.contains("idx_event_outbox_plugin_dispatch_expired_lease"));
        assert!(migration.contains("event_type = 'plugin.hook.dispatch'"));
        assert!(migration.contains("status in ('pending', 'failed')"));
        assert!(migration.contains("status = 'delivering'"));
        assert!(migration.contains("attempts < max_attempts"));
    }

    // Live-DB smoke: validates the plugin dispatch claim SQL parses and plans
    // against the migrated schema. Plain EXPLAIN does not execute the UPDATE,
    // so it does not claim or mutate queued dispatch events.
    //   cargo test -- --ignored plugin_dispatch_claim_sql_plans_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn plugin_dispatch_claim_sql_plans_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        for index_name in [
            "idx_event_outbox_plugin_dispatch_available",
            "idx_event_outbox_plugin_dispatch_expired_lease",
        ] {
            let index_def = sqlx::query_scalar::<_, Option<String>>(
                r#"
                select indexdef
                from pg_indexes
                where schemaname = 'public'
                  and indexname = $1
                "#,
            )
            .bind(index_name)
            .fetch_one(&pool)
            .await
            .expect("read plugin dispatch claim index")
            .unwrap_or_else(|| panic!("{index_name} should exist"));
            assert!(index_def.contains("event_type = 'plugin.hook.dispatch'::text"));
            assert!(index_def.contains("attempts < max_attempts"));
        }

        let plan_rows = sqlx::query(&format!("explain {CLAIM_NEXT_PLUGIN_DISPATCH_SQL}"))
            .bind(PLUGIN_HOOK_DISPATCH_EVENT)
            .bind(PLUGIN_EXECUTOR_ID)
            .bind(dispatch_lease_seconds(5_000) as i64)
            .fetch_all(&pool)
            .await
            .expect("plugin dispatch claim SQL should parse and plan");
        assert!(
            !plan_rows.is_empty(),
            "EXPLAIN should return a query plan for plugin dispatch claim"
        );
    }

    #[test]
    fn plugin_execution_package_public_id_input_keeps_uuid_index_shape() {
        let execution = include_str!("execution.rs");
        let bad_package_filter = format!("{}{}", "pkg.public_id::text = ", "$2");

        assert!(execution.contains("and pkg.public_id = case"));
        assert!(execution.contains("then $2::uuid"));
        assert!(!execution.contains(&bad_package_filter));
    }

    #[test]
    fn stale_execution_recovery_summary_reports_work() {
        assert!(!PluginStaleExecutionRecoverySummary::default().recovered_anything());
        assert!(
            PluginStaleExecutionRecoverySummary {
                expired_runs: 1,
                revoked_tokens: 0,
            }
            .recovered_anything()
        );
    }

    // Live-DB smoke: validates stale plugin execution recovery parses and
    // plans against the migrated schema. Plain EXPLAIN does not execute the
    // UPDATE, so this does not mutate any execution runs or Host Tokens.
    //   cargo test -- --ignored stale_plugin_execution_recovery_sql_plans_against_live_schema
    #[tokio::test]
    #[ignore = "requires a running PostgreSQL from ./scripts/dev-deps.ps1"]
    async fn stale_plugin_execution_recovery_sql_plans_against_live_schema() {
        use sqlx::postgres::PgPoolOptions;

        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://fbz:fbz@127.0.0.1:5432/fbz".to_owned());
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to live PostgreSQL");
        crate::db::migrate(&pool).await.expect("run migrations");

        let plan_rows = sqlx::query(&format!("explain {EXPIRE_STALE_PLUGIN_EXECUTIONS_SQL}"))
            .bind(STALE_EXECUTION_MESSAGE)
            .fetch_all(&pool)
            .await
            .expect("stale plugin execution recovery SQL should parse and plan");
        assert!(
            !plan_rows.is_empty(),
            "EXPLAIN should return a query plan for stale plugin execution recovery"
        );
    }

    #[test]
    fn duration_millis_clamps_to_i32() {
        assert_eq!(duration_millis_i32(Duration::from_millis(42)), 42);
        assert_eq!(
            duration_millis_i32(Duration::from_millis(i32::MAX as u64 + 1)),
            i32::MAX
        );
    }

    #[test]
    fn limited_response_body_accumulation_rejects_oversized_chunks() {
        let mut body = Vec::new();
        append_limited_response_chunk(&mut body, &Bytes::from_static(b"abc"), 5).unwrap();

        let err =
            append_limited_response_chunk(&mut body, &Bytes::from_static(b"def"), 5).unwrap_err();

        assert_eq!(body, b"abc");
        assert!(matches!(
            err,
            PluginRuntimeError::ResponseTooLarge { limit: 5 }
        ));
    }

    #[test]
    fn limited_response_body_accumulation_allows_exact_limit() {
        let mut body = Vec::new();

        append_limited_response_chunk(&mut body, &Bytes::from_static(b"abc"), 6).unwrap();
        append_limited_response_chunk(&mut body, &Bytes::from_static(b"def"), 6).unwrap();

        assert_eq!(body, b"abcdef");
    }

    #[test]
    fn truncation_preserves_utf8_boundary() {
        let value = "通知通知通知";

        assert_eq!(truncate_str(value, 7), "通知");
    }

    #[test]
    fn plugin_http_entrypoint_accepts_http_and_https() {
        let http_uri = validate_http_entrypoint("http://127.0.0.1:8080/hook").unwrap();
        let https_uri = validate_http_entrypoint("https://plugins.example.test/hook").unwrap();

        assert_eq!(http_uri.scheme_str(), Some("http"));
        assert_eq!(https_uri.scheme_str(), Some("https"));
    }

    #[test]
    fn plugin_http_host_allowlist_supports_exact_and_suffix_matches() {
        let allowed_hosts = vec![
            "127.0.0.1".to_owned(),
            "plugins.internal".to_owned(),
            "*.example.test".to_owned(),
        ];

        assert!(http_host_allowed("127.0.0.1", &allowed_hosts));
        assert!(http_host_allowed("Plugins.Internal", &allowed_hosts));
        assert!(http_host_allowed("notify.example.test", &allowed_hosts));
        assert!(http_host_allowed(
            "deep.notify.example.test",
            &allowed_hosts
        ));
        assert!(!http_host_allowed("example.test", &allowed_hosts));
        assert!(!http_host_allowed("evil-example.test", &allowed_hosts));
    }

    #[test]
    fn plugin_http_entrypoint_rejects_hosts_outside_allowlist() {
        let uri = validate_http_entrypoint("https://plugins.example.test/hook").unwrap();
        let err = ensure_http_host_allowed(&uri, &["localhost".to_owned()]).unwrap_err();

        assert!(matches!(
            err,
            PluginRuntimeError::HostNotAllowed(host) if host == "plugins.example.test"
        ));
    }

    #[test]
    fn plugin_http_signature_headers_bind_plugin_timestamp_and_body() {
        let headers = plugin_http_signature_headers(
            "abcdef0123456789abcdef0123456789",
            "dev.fbz.notify",
            "dispatch-1",
            1_717_171_717,
            br#"{"event":"scan"}"#,
        );
        let same = plugin_http_signature_headers(
            "abcdef0123456789abcdef0123456789",
            "dev.fbz.notify",
            "dispatch-1",
            1_717_171_717,
            br#"{"event":"scan"}"#,
        );
        let other_plugin = plugin_http_signature_headers(
            "abcdef0123456789abcdef0123456789",
            "dev.fbz.other",
            "dispatch-1",
            1_717_171_717,
            br#"{"event":"scan"}"#,
        );
        let other_idempotency_key = plugin_http_signature_headers(
            "abcdef0123456789abcdef0123456789",
            "dev.fbz.notify",
            "dispatch-2",
            1_717_171_717,
            br#"{"event":"scan"}"#,
        );
        let other_body = plugin_http_signature_headers(
            "abcdef0123456789abcdef0123456789",
            "dev.fbz.notify",
            "dispatch-1",
            1_717_171_717,
            br#"{"event":"other"}"#,
        );

        assert_eq!(headers.version, "v1");
        assert_eq!(headers.timestamp, "1717171717");
        assert_eq!(headers.body_sha256.len(), 64);
        assert!(headers.signature.starts_with("sha256="));
        assert_eq!(headers.signature, same.signature);
        assert_ne!(headers.signature, other_plugin.signature);
        assert_ne!(headers.signature, other_idempotency_key.signature);
        assert_ne!(headers.signature, other_body.signature);
    }

    #[test]
    fn hmac_sha256_matches_rfc_4231_vector() {
        assert_eq!(
            hmac_sha256_hex(&[0x0b; 20], b"Hi There"),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7".to_owned()
        );
    }

    #[test]
    fn plugin_http_entrypoint_rejects_unsupported_or_hostless_urls() {
        assert!(matches!(
            validate_http_entrypoint("ftp://plugins.example.test/hook"),
            Err(PluginRuntimeError::UnsupportedScheme(scheme)) if scheme == "ftp"
        ));
        assert!(validate_http_entrypoint("https:///hook").is_err());
    }

    #[test]
    fn runtime_error_exposes_http_status_for_audit() {
        let err = PluginRuntimeError::HttpStatus {
            status: StatusCode::BAD_GATEWAY.as_u16() as i32,
            body: "bad gateway".to_owned(),
        };

        assert_eq!(err.response_status(), Some(502));
        assert_eq!(err.response_body(), Some("bad gateway"));
    }
}
