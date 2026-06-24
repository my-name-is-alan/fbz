use std::{
    collections::HashMap,
    error::Error,
    fmt::{Display, Formatter},
    time::{Duration, Instant},
};

use reqwest::{
    Client, Response,
    header::{CONTENT_TYPE, HeaderMap, USER_AGENT},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use tracing::warn;

use crate::{
    config::{NotificationWorkerConfig, SecretConfig},
    db::DbPool,
    notifications::{
        secrets::{SECRET_ALGORITHM, SecretCipher, contains_secret_refs, materialize_secret_refs},
        target_config::{
            DEFAULT_TELEGRAM_API_BASE_URL, NotificationTargetConfigError, optional_config_string,
            optional_webhook_headers, required_config_string, validate_delivery_url,
        },
    },
};

pub const NOTIFICATION_REQUESTED_EVENT: &str = "notification.send.requested";

const NOTIFICATION_WORKER_ID: &str = "fbz-api-notification-worker";
const MAX_TARGETS_PER_NOTIFICATION: i64 = 100;
const MAX_RESPONSE_BODY_BYTES: usize = 4096;
const MAX_ERROR_BYTES: usize = 2048;

#[derive(Clone)]
pub struct NotificationDeliveryService {
    pool: DbPool,
    config: NotificationWorkerConfig,
    secret_cipher: Option<SecretCipher>,
    client: NotificationHttpClient,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationDeliverySummary {
    pub outbox_event_id: String,
    pub request_id: String,
    pub target_count: usize,
    pub delivered_targets: usize,
    pub failed_targets: usize,
    pub outbox_status: String,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct ClaimedNotificationEvent {
    id: i64,
    public_id: String,
    payload: Value,
    attempt: i32,
    max_attempts: i32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct NotificationRequestedPayload {
    request_id: String,
    plugin_id: String,
    package_id: String,
    title: String,
    message: String,
    level: String,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    metadata: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NotificationRequestRecord {
    id: i64,
    public_id: String,
}

#[derive(Clone, Debug, PartialEq)]
struct NotificationTarget {
    id: i64,
    public_id: String,
    name: String,
    target_type: String,
    config: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NotificationRuntimeOutput {
    response_status: i32,
    response_body: String,
}

#[derive(Clone, Debug, PartialEq)]
struct DeliveryHttpRequest {
    url: String,
    body: Value,
    headers: HeaderMap,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotificationDeliveryError {
    Database(String),
    Secret(String),
}

#[derive(Debug)]
enum NotificationRuntimeError {
    InvalidConfig(String),
    InvalidHeader(String),
    Request(String),
    HttpStatus { status: i32, body: String },
    UnsupportedTargetType(String),
}

impl NotificationDeliveryService {
    pub fn new(
        pool: DbPool,
        config: NotificationWorkerConfig,
        secret_config: SecretConfig,
    ) -> Self {
        Self {
            pool,
            config,
            secret_cipher: SecretCipher::from_config(&secret_config).ok(),
            client: NotificationHttpClient::new(),
        }
    }

    pub async fn run_next_delivery(
        &self,
    ) -> Result<Option<NotificationDeliverySummary>, NotificationDeliveryError> {
        let Some(event) = self.claim_next_event().await? else {
            return Ok(None);
        };

        let payload = match parse_notification_payload(&event.payload) {
            Ok(payload) => payload,
            Err(err) => {
                let message = truncate_error(&err);
                let outbox_status = self.mark_event_failure(&event, &message).await?;
                return Ok(Some(NotificationDeliverySummary {
                    outbox_event_id: event.public_id,
                    request_id: String::new(),
                    target_count: 0,
                    delivered_targets: 0,
                    failed_targets: 0,
                    outbox_status,
                    error_message: Some(message),
                }));
            }
        };

        let Some(request) = self.load_request(&payload.request_id).await? else {
            let message = format!(
                "notification request `{}` was not found",
                payload.request_id
            );
            let outbox_status = self.mark_event_failure(&event, &message).await?;
            return Ok(Some(NotificationDeliverySummary {
                outbox_event_id: event.public_id,
                request_id: payload.request_id,
                target_count: 0,
                delivered_targets: 0,
                failed_targets: 0,
                outbox_status,
                error_message: Some(message),
            }));
        };

        self.mark_request_status(request.id, "delivering", None)
            .await?;
        let targets = self.load_targets(payload.channel.as_deref()).await?;
        if targets.is_empty() {
            let message = no_targets_message(payload.channel.as_deref());
            let outbox_status = self.mark_event_failure(&event, &message).await?;
            self.mark_request_status(
                request.id,
                request_status_for_outbox_failure(&outbox_status),
                Some(&message),
            )
            .await?;
            return Ok(Some(NotificationDeliverySummary {
                outbox_event_id: event.public_id,
                request_id: request.public_id,
                target_count: 0,
                delivered_targets: 0,
                failed_targets: 0,
                outbox_status,
                error_message: Some(message),
            }));
        }

        let mut delivered_targets = 0;
        let mut failed_targets = 0;
        let mut last_error = None;
        for target in &targets {
            if self.has_successful_attempt(request.id, target.id).await? {
                delivered_targets += 1;
                continue;
            }

            let attempt_id = self.create_attempt(&request, &event, target).await?;
            let started = Instant::now();
            let result = self
                .client
                .deliver(
                    target,
                    &payload,
                    Duration::from_millis(self.config.delivery_timeout_ms),
                )
                .await;

            match result {
                Ok(output) => {
                    self.finish_attempt_success(attempt_id, started.elapsed(), &output)
                        .await?;
                    self.mark_target_success(target.id).await?;
                    delivered_targets += 1;
                }
                Err(err) => {
                    let message = truncate_error(&err.to_string());
                    self.finish_attempt_failure(attempt_id, started.elapsed(), &err, &message)
                        .await?;
                    self.mark_target_failure(target.id, &message).await?;
                    failed_targets += 1;
                    last_error = Some(message);
                }
            }
        }

        if failed_targets == 0 {
            self.mark_request_status(request.id, "delivered", None)
                .await?;
            self.mark_event_delivered(event.id).await?;
            return Ok(Some(NotificationDeliverySummary {
                outbox_event_id: event.public_id,
                request_id: request.public_id,
                target_count: targets.len(),
                delivered_targets,
                failed_targets,
                outbox_status: "delivered".to_owned(),
                error_message: None,
            }));
        }

        let message = format!(
            "{} notification target(s) failed{}",
            failed_targets,
            last_error
                .as_deref()
                .map(|error| format!(": {error}"))
                .unwrap_or_default()
        );
        let outbox_status = self.mark_event_failure(&event, &message).await?;
        self.mark_request_status(
            request.id,
            request_status_for_outbox_failure(&outbox_status),
            Some(&message),
        )
        .await?;

        Ok(Some(NotificationDeliverySummary {
            outbox_event_id: event.public_id,
            request_id: request.public_id,
            target_count: targets.len(),
            delivered_targets,
            failed_targets,
            outbox_status,
            error_message: Some(message),
        }))
    }

    async fn claim_next_event(
        &self,
    ) -> Result<Option<ClaimedNotificationEvent>, NotificationDeliveryError> {
        let row = sqlx::query(
            r#"
            update event_outbox
            set status = 'delivering',
                attempts = attempts + 1,
                locked_by = $2,
                locked_until = now() + interval '5 minutes',
                last_error = null
            where id = (
                select id
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
            returning
                id,
                public_id::text as public_id,
                payload,
                attempts,
                max_attempts
            "#,
        )
        .bind(NOTIFICATION_REQUESTED_EVENT)
        .bind(NOTIFICATION_WORKER_ID)
        .fetch_optional(&self.pool)
        .await
        .map_err(NotificationDeliveryError::from)?;

        row.map(|row| -> Result<ClaimedNotificationEvent, sqlx::Error> {
            Ok(ClaimedNotificationEvent {
                id: row.try_get("id")?,
                public_id: row.try_get("public_id")?,
                payload: row.try_get("payload")?,
                attempt: row.try_get("attempts")?,
                max_attempts: row.try_get("max_attempts")?,
            })
        })
        .transpose()
        .map_err(NotificationDeliveryError::from)
    }

    async fn load_request(
        &self,
        request_id: &str,
    ) -> Result<Option<NotificationRequestRecord>, NotificationDeliveryError> {
        let row = sqlx::query(
            r#"
            select id,
                   public_id::text as public_id
            from plugin_notification_requests
            where public_id = case
                when $1::text ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
                then $1::uuid
                else null::uuid
            end
            "#,
        )
        .bind(request_id.trim())
        .fetch_optional(&self.pool)
        .await
        .map_err(NotificationDeliveryError::from)?;

        row.map(|row| -> Result<NotificationRequestRecord, sqlx::Error> {
            Ok(NotificationRequestRecord {
                id: row.try_get("id")?,
                public_id: row.try_get("public_id")?,
            })
        })
        .transpose()
        .map_err(NotificationDeliveryError::from)
    }

    async fn load_targets(
        &self,
        channel: Option<&str>,
    ) -> Result<Vec<NotificationTarget>, NotificationDeliveryError> {
        let rows = sqlx::query(
            r#"
            select id,
                   public_id::text as public_id,
                   name,
                   target_type,
                   config
            from notification_targets
            where is_enabled = true
              and (
                  ($1::text is null and channel is null)
                  or channel = $1
              )
            order by target_type, name, id
            limit $2
            "#,
        )
        .bind(channel)
        .bind(MAX_TARGETS_PER_NOTIFICATION)
        .fetch_all(&self.pool)
        .await
        .map_err(NotificationDeliveryError::from)?;

        let mut targets = Vec::with_capacity(rows.len());
        for row in rows {
            let target_id = row.try_get("id").map_err(NotificationDeliveryError::from)?;
            let config = row
                .try_get::<Value, _>("config")
                .map_err(NotificationDeliveryError::from)?;
            let config = self.materialize_target_config(target_id, config).await?;
            targets.push(NotificationTarget {
                id: target_id,
                public_id: row
                    .try_get("public_id")
                    .map_err(NotificationDeliveryError::from)?,
                name: row
                    .try_get("name")
                    .map_err(NotificationDeliveryError::from)?,
                target_type: row
                    .try_get("target_type")
                    .map_err(NotificationDeliveryError::from)?,
                config,
            });
        }

        Ok(targets)
    }

    async fn materialize_target_config(
        &self,
        target_id: i64,
        config: Value,
    ) -> Result<Value, NotificationDeliveryError> {
        if !contains_secret_refs(&config) {
            return Ok(config);
        }

        let secrets = self.load_target_secret_values(target_id).await?;
        materialize_secret_refs(&config, &secrets)
            .map_err(|err| NotificationDeliveryError::Secret(err.to_string()))
    }

    async fn load_target_secret_values(
        &self,
        target_id: i64,
    ) -> Result<HashMap<String, String>, NotificationDeliveryError> {
        let Some(cipher) = &self.secret_cipher else {
            return Err(NotificationDeliveryError::Secret(
                "FBZ_SECRET_KEY is required to read notification target secrets".to_owned(),
            ));
        };
        let rows = sqlx::query(
            r#"
            select secret_key,
                   algorithm,
                   nonce,
                   ciphertext
            from notification_target_secrets
            where target_id = $1
            order by secret_key
            "#,
        )
        .bind(target_id)
        .fetch_all(&self.pool)
        .await
        .map_err(NotificationDeliveryError::from)?;

        let mut secrets = HashMap::with_capacity(rows.len());
        for row in rows {
            let secret_key = row
                .try_get::<String, _>("secret_key")
                .map_err(NotificationDeliveryError::from)?;
            let algorithm = row
                .try_get::<String, _>("algorithm")
                .map_err(NotificationDeliveryError::from)?;
            if algorithm != SECRET_ALGORITHM {
                return Err(NotificationDeliveryError::Secret(format!(
                    "unsupported notification target secret algorithm `{algorithm}`"
                )));
            }
            let nonce = row
                .try_get::<Vec<u8>, _>("nonce")
                .map_err(NotificationDeliveryError::from)?;
            let ciphertext = row
                .try_get::<Vec<u8>, _>("ciphertext")
                .map_err(NotificationDeliveryError::from)?;
            let value = cipher
                .decrypt(target_id, &secret_key, &nonce, &ciphertext)
                .map_err(|err| NotificationDeliveryError::Secret(err.to_string()))?;
            secrets.insert(secret_key, value);
        }

        Ok(secrets)
    }

    async fn has_successful_attempt(
        &self,
        request_id: i64,
        target_id: i64,
    ) -> Result<bool, NotificationDeliveryError> {
        let row = sqlx::query(
            r#"
            select exists (
                select 1
                from notification_delivery_attempts
                where notification_request_id = $1
                  and target_id = $2
                  and status = 'succeeded'
            ) as found
            "#,
        )
        .bind(request_id)
        .bind(target_id)
        .fetch_one(&self.pool)
        .await
        .map_err(NotificationDeliveryError::from)?;

        row.try_get("found")
            .map_err(NotificationDeliveryError::from)
    }

    async fn create_attempt(
        &self,
        request: &NotificationRequestRecord,
        event: &ClaimedNotificationEvent,
        target: &NotificationTarget,
    ) -> Result<i64, NotificationDeliveryError> {
        let row = sqlx::query(
            r#"
            insert into notification_delivery_attempts (
                notification_request_id,
                outbox_event_id,
                target_id,
                target_public_id,
                target_type,
                target_name,
                attempt,
                status
            )
            values ($1, $2, $3, $4::uuid, $5, $6, $7, 'running')
            returning id
            "#,
        )
        .bind(request.id)
        .bind(event.id)
        .bind(target.id)
        .bind(&target.public_id)
        .bind(target.target_type.trim())
        .bind(target.name.trim())
        .bind(event.attempt)
        .fetch_one(&self.pool)
        .await
        .map_err(NotificationDeliveryError::from)?;

        row.try_get("id").map_err(NotificationDeliveryError::from)
    }

    async fn finish_attempt_success(
        &self,
        attempt_id: i64,
        elapsed: Duration,
        output: &NotificationRuntimeOutput,
    ) -> Result<(), NotificationDeliveryError> {
        sqlx::query(
            r#"
            update notification_delivery_attempts
            set status = 'succeeded',
                response_status = $2,
                response_body = $3,
                error_message = null,
                duration_ms = $4,
                finished_at = now()
            where id = $1
            "#,
        )
        .bind(attempt_id)
        .bind(output.response_status)
        .bind(&output.response_body)
        .bind(duration_millis_i32(elapsed))
        .execute(&self.pool)
        .await
        .map_err(NotificationDeliveryError::from)?;

        Ok(())
    }

    async fn finish_attempt_failure(
        &self,
        attempt_id: i64,
        elapsed: Duration,
        err: &NotificationRuntimeError,
        message: &str,
    ) -> Result<(), NotificationDeliveryError> {
        sqlx::query(
            r#"
            update notification_delivery_attempts
            set status = 'failed',
                response_status = $2,
                response_body = $3,
                error_message = $4,
                duration_ms = $5,
                finished_at = now()
            where id = $1
            "#,
        )
        .bind(attempt_id)
        .bind(err.response_status())
        .bind(err.response_body())
        .bind(message)
        .bind(duration_millis_i32(elapsed))
        .execute(&self.pool)
        .await
        .map_err(NotificationDeliveryError::from)?;

        Ok(())
    }

    async fn mark_target_success(&self, target_id: i64) -> Result<(), NotificationDeliveryError> {
        sqlx::query(
            r#"
            update notification_targets
            set delivery_count = delivery_count + 1,
                last_success_at = now(),
                last_error = null,
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(target_id)
        .execute(&self.pool)
        .await
        .map_err(NotificationDeliveryError::from)?;

        Ok(())
    }

    async fn mark_target_failure(
        &self,
        target_id: i64,
        message: &str,
    ) -> Result<(), NotificationDeliveryError> {
        sqlx::query(
            r#"
            update notification_targets
            set failure_count = failure_count + 1,
                last_failure_at = now(),
                last_error = $2,
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(target_id)
        .bind(message)
        .execute(&self.pool)
        .await
        .map_err(NotificationDeliveryError::from)?;

        Ok(())
    }

    async fn mark_request_status(
        &self,
        request_id: i64,
        status: &str,
        last_error: Option<&str>,
    ) -> Result<(), NotificationDeliveryError> {
        sqlx::query(
            r#"
            update plugin_notification_requests
            set status = $2,
                last_error = $3,
                updated_at = now()
            where id = $1
            "#,
        )
        .bind(request_id)
        .bind(status)
        .bind(last_error)
        .execute(&self.pool)
        .await
        .map_err(NotificationDeliveryError::from)?;

        Ok(())
    }

    async fn mark_event_delivered(&self, event_id: i64) -> Result<(), NotificationDeliveryError> {
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
        .map_err(NotificationDeliveryError::from)?;

        Ok(())
    }

    async fn mark_event_failure(
        &self,
        event: &ClaimedNotificationEvent,
        message: &str,
    ) -> Result<String, NotificationDeliveryError> {
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
        .map_err(NotificationDeliveryError::from)?;

        if outbox_status == "failed" {
            warn!(
                event_id = event.id,
                outbox_event_id = %event.public_id,
                attempt = event.attempt,
                max_attempts = event.max_attempts,
                retry_delay_seconds,
                error = %message,
                "notification delivery failed; scheduled retry"
            );
        }

        Ok(outbox_status.to_owned())
    }
}

#[derive(Clone)]
struct NotificationHttpClient {
    client: Client,
}

impl NotificationHttpClient {
    fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    async fn deliver(
        &self,
        target: &NotificationTarget,
        payload: &NotificationRequestedPayload,
        timeout_duration: Duration,
    ) -> Result<NotificationRuntimeOutput, NotificationRuntimeError> {
        let request = build_delivery_request(target, payload)?;
        let mut builder = self
            .client
            .post(&request.url)
            .timeout(timeout_duration)
            .header(CONTENT_TYPE, "application/json")
            .header(USER_AGENT, "fbz-api-notification-worker/0.1")
            .json(&request.body);

        for (name, value) in request.headers.iter() {
            builder = builder.header(name, value);
        }

        let response = builder
            .send()
            .await
            .map_err(|err| NotificationRuntimeError::Request(err.to_string()))?;
        let status = response.status();
        let response_body = read_limited_response(response).await?;

        if !status.is_success() {
            return Err(NotificationRuntimeError::HttpStatus {
                status: status.as_u16() as i32,
                body: response_body,
            });
        }

        Ok(NotificationRuntimeOutput {
            response_status: status.as_u16() as i32,
            response_body,
        })
    }
}

async fn read_limited_response(mut response: Response) -> Result<String, NotificationRuntimeError> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|err| NotificationRuntimeError::Request(err.to_string()))?
    {
        let remaining = MAX_RESPONSE_BODY_BYTES.saturating_sub(body.len());
        if remaining == 0 {
            break;
        }

        body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }

    Ok(String::from_utf8_lossy(&body).to_string())
}

fn build_delivery_request(
    target: &NotificationTarget,
    payload: &NotificationRequestedPayload,
) -> Result<DeliveryHttpRequest, NotificationRuntimeError> {
    match target.target_type.trim() {
        "webhook" => build_webhook_request(target, payload),
        "telegram" => build_telegram_request(target, payload),
        "wecom" => build_wecom_request(target, payload),
        "plugin" => Err(NotificationRuntimeError::UnsupportedTargetType(
            "plugin notification targets are reserved for a future plugin bridge".to_owned(),
        )),
        other => Err(NotificationRuntimeError::UnsupportedTargetType(
            other.to_owned(),
        )),
    }
}

fn build_webhook_request(
    target: &NotificationTarget,
    payload: &NotificationRequestedPayload,
) -> Result<DeliveryHttpRequest, NotificationRuntimeError> {
    let url = required_config_string(&target.config, "url")?;
    validate_delivery_url(&url)?;

    Ok(DeliveryHttpRequest {
        url,
        body: serde_json::to_value(payload)
            .map_err(|err| NotificationRuntimeError::InvalidConfig(err.to_string()))?,
        headers: optional_webhook_headers(&target.config)?,
    })
}

fn build_telegram_request(
    target: &NotificationTarget,
    payload: &NotificationRequestedPayload,
) -> Result<DeliveryHttpRequest, NotificationRuntimeError> {
    let bot_token = required_config_string(&target.config, "botToken")?;
    let chat_id = required_config_string(&target.config, "chatId")?;
    let api_base_url = optional_config_string(&target.config, "apiBaseUrl")
        .unwrap_or_else(|| DEFAULT_TELEGRAM_API_BASE_URL.to_owned());
    validate_delivery_url(&api_base_url)?;

    Ok(DeliveryHttpRequest {
        url: format!(
            "{}/bot{}/sendMessage",
            api_base_url.trim_end_matches('/'),
            bot_token
        ),
        body: json!({
            "chat_id": chat_id,
            "text": truncate_str(&notification_text(payload), 4096),
            "disable_web_page_preview": true
        }),
        headers: HeaderMap::new(),
    })
}

fn build_wecom_request(
    target: &NotificationTarget,
    payload: &NotificationRequestedPayload,
) -> Result<DeliveryHttpRequest, NotificationRuntimeError> {
    let url = required_config_string(&target.config, "webhookUrl")?;
    validate_delivery_url(&url)?;

    Ok(DeliveryHttpRequest {
        url,
        body: json!({
            "msgtype": "text",
            "text": {
                "content": truncate_str(&notification_text(payload), 4096)
            }
        }),
        headers: HeaderMap::new(),
    })
}

fn parse_notification_payload(payload: &Value) -> Result<NotificationRequestedPayload, String> {
    let parsed: NotificationRequestedPayload =
        serde_json::from_value(payload.clone()).map_err(|err| err.to_string())?;
    validate_non_empty("requestId", &parsed.request_id)?;
    validate_non_empty("pluginId", &parsed.plugin_id)?;
    validate_non_empty("packageId", &parsed.package_id)?;
    validate_non_empty("title", &parsed.title)?;
    validate_non_empty("message", &parsed.message)?;
    match parsed.level.as_str() {
        "info" | "success" | "warning" | "error" => {}
        _ => {
            return Err("level must be one of info, success, warning, or error".to_owned());
        }
    }
    if let Some(channel) = &parsed.channel {
        validate_channel(channel)?;
    }
    if !parsed.metadata.is_object() {
        return Err("metadata must be a JSON object".to_owned());
    }

    Ok(parsed)
}

fn validate_non_empty(field: &'static str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} is required"));
    }
    Ok(())
}

fn validate_channel(value: &str) -> Result<(), String> {
    let channel = value.trim();
    if channel.is_empty() {
        return Err("channel must not be empty".to_owned());
    }
    if channel.len() > 64 {
        return Err("channel must be at most 64 characters".to_owned());
    }
    if !channel
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':'))
    {
        return Err(
            "channel may only contain letters, numbers, dot, dash, underscore, or colon".to_owned(),
        );
    }
    Ok(())
}

fn notification_text(payload: &NotificationRequestedPayload) -> String {
    format!(
        "[{}] {}\n{}",
        payload.level.to_ascii_uppercase(),
        payload.title.trim(),
        payload.message.trim()
    )
}

fn no_targets_message(channel: Option<&str>) -> String {
    match channel {
        Some(channel) => format!("no enabled notification targets for channel `{channel}`"),
        None => "no enabled global notification targets".to_owned(),
    }
}

fn request_status_for_outbox_failure(outbox_status: &str) -> &'static str {
    if outbox_status == "discarded" {
        "discarded"
    } else {
        "failed"
    }
}

fn retry_delay_seconds(attempt: i32) -> i64 {
    let bounded_attempt = attempt.clamp(1, 6) as u32;
    5_i64 * 2_i64.pow(bounded_attempt - 1)
}

fn duration_millis_i32(duration: Duration) -> i32 {
    duration.as_millis().min(i32::MAX as u128) as i32
}

fn truncate_error(message: &str) -> String {
    truncate_str(message, MAX_ERROR_BYTES)
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

impl NotificationRuntimeError {
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

impl From<NotificationTargetConfigError> for NotificationRuntimeError {
    fn from(error: NotificationTargetConfigError) -> Self {
        match error {
            NotificationTargetConfigError::InvalidConfig(message) => Self::InvalidConfig(message),
            NotificationTargetConfigError::InvalidHeader(message) => Self::InvalidHeader(message),
            NotificationTargetConfigError::UnsupportedTargetType(target_type) => {
                Self::UnsupportedTargetType(target_type)
            }
        }
    }
}

impl Display for NotificationRuntimeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidConfig(message) => {
                write!(f, "invalid notification target config: {message}")
            }
            Self::InvalidHeader(message) => {
                write!(f, "invalid notification target header: {message}")
            }
            Self::Request(message) => write!(f, "notification request failed: {message}"),
            Self::HttpStatus { status, body } => {
                write!(f, "notification target returned {status}: {body}")
            }
            Self::UnsupportedTargetType(target_type) => {
                write!(f, "unsupported notification target type: {target_type}")
            }
        }
    }
}

impl Error for NotificationRuntimeError {}

impl From<sqlx::Error> for NotificationDeliveryError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error.to_string())
    }
}

impl Display for NotificationDeliveryError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(message) => write!(f, "database error: {message}"),
            Self::Secret(message) => write!(f, "secret error: {message}"),
        }
    }
}

impl Error for NotificationDeliveryError {}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use super::*;

    fn payload() -> NotificationRequestedPayload {
        NotificationRequestedPayload {
            request_id: "request-1".to_owned(),
            plugin_id: "dev.fbz.notify".to_owned(),
            package_id: "package-1".to_owned(),
            title: "Scan complete".to_owned(),
            message: "2 new items".to_owned(),
            level: "success".to_owned(),
            channel: Some("tg.primary".to_owned()),
            metadata: json!({ "libraryId": "library-1" }),
        }
    }

    #[test]
    fn parse_notification_payload_requires_core_fields() {
        let parsed = parse_notification_payload(&json!({
            "requestId": "request-1",
            "pluginId": "dev.fbz.notify",
            "packageId": "package-1",
            "title": "Scan complete",
            "message": "2 new items",
            "level": "success",
            "channel": "tg.primary",
            "metadata": {}
        }))
        .unwrap();

        assert_eq!(parsed.request_id, "request-1");
        assert_eq!(parsed.channel.as_deref(), Some("tg.primary"));

        assert!(
            parse_notification_payload(&json!({
                "requestId": "",
                "pluginId": "dev.fbz.notify",
                "packageId": "package-1",
                "title": "Scan complete",
                "message": "2 new items",
                "level": "success",
                "metadata": {}
            }))
            .is_err()
        );
    }

    #[test]
    fn parse_notification_payload_rejects_invalid_level_channel_or_metadata() {
        assert!(
            parse_notification_payload(&json!({
                "requestId": "request-1",
                "pluginId": "dev.fbz.notify",
                "packageId": "package-1",
                "title": "Scan complete",
                "message": "2 new items",
                "level": "panic",
                "metadata": {}
            }))
            .is_err()
        );

        assert!(
            parse_notification_payload(&json!({
                "requestId": "request-1",
                "pluginId": "dev.fbz.notify",
                "packageId": "package-1",
                "title": "Scan complete",
                "message": "2 new items",
                "level": "success",
                "channel": "../bad",
                "metadata": {}
            }))
            .is_err()
        );

        assert!(
            parse_notification_payload(&json!({
                "requestId": "request-1",
                "pluginId": "dev.fbz.notify",
                "packageId": "package-1",
                "title": "Scan complete",
                "message": "2 new items",
                "level": "success",
                "metadata": []
            }))
            .is_err()
        );
    }

    #[test]
    fn notification_request_public_id_input_keeps_uuid_index_shape() {
        let delivery = include_str!("delivery.rs");
        let bad_request_filter = format!("{}{}", "where public_id::text = ", "$1");

        assert!(delivery.contains("from plugin_notification_requests"));
        assert!(delivery.contains("where public_id = case"));
        assert!(delivery.contains("then $1::uuid"));
        assert!(!delivery.contains(&bad_request_filter));
    }

    #[test]
    fn webhook_request_preserves_payload_and_headers() {
        let target = NotificationTarget {
            id: 1,
            public_id: "target-1".to_owned(),
            name: "Webhook".to_owned(),
            target_type: "webhook".to_owned(),
            config: json!({
                "url": "http://127.0.0.1:9000/notify",
                "headers": {
                    "x-notify-key": "local"
                }
            }),
        };

        let request = build_delivery_request(&target, &payload()).unwrap();

        assert_eq!(request.url, "http://127.0.0.1:9000/notify");
        assert_eq!(request.body["requestId"], "request-1");
        assert_eq!(request.headers["x-notify-key"], "local");
    }

    #[tokio::test]
    async fn http_client_posts_webhook_payload() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut buffer = Vec::new();
            let mut chunk = [0_u8; 1024];
            loop {
                let read = stream.read(&mut chunk).unwrap();
                if read == 0 {
                    break;
                }
                buffer.extend_from_slice(&chunk[..read]);
                if request_body_is_complete(&buffer) {
                    break;
                }
            }
            stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok")
                .unwrap();
            String::from_utf8_lossy(&buffer).to_string()
        });
        let target = NotificationTarget {
            id: 1,
            public_id: "target-1".to_owned(),
            name: "Webhook".to_owned(),
            target_type: "webhook".to_owned(),
            config: json!({
                "url": format!("http://{addr}/notify")
            }),
        };

        let output = NotificationHttpClient::new()
            .deliver(&target, &payload(), Duration::from_secs(5))
            .await
            .unwrap();
        let request = server.join().unwrap();

        assert_eq!(output.response_status, 200);
        assert_eq!(output.response_body, "ok");
        assert!(request.starts_with("POST /notify HTTP/1.1"));
        assert!(request.contains(r#""requestId":"request-1""#));
        assert!(request.contains(r#""title":"Scan complete""#));
    }

    #[test]
    fn telegram_request_uses_configurable_api_base() {
        let target = NotificationTarget {
            id: 1,
            public_id: "target-1".to_owned(),
            name: "Telegram".to_owned(),
            target_type: "telegram".to_owned(),
            config: json!({
                "apiBaseUrl": "http://127.0.0.1:9000/tg",
                "botToken": "bot-token",
                "chatId": "chat-1"
            }),
        };

        let request = build_delivery_request(&target, &payload()).unwrap();

        assert_eq!(
            request.url,
            "http://127.0.0.1:9000/tg/botbot-token/sendMessage"
        );
        assert_eq!(request.body["chat_id"], "chat-1");
        assert!(
            request.body["text"]
                .as_str()
                .unwrap()
                .contains("Scan complete")
        );
    }

    #[test]
    fn wecom_request_uses_text_payload() {
        let target = NotificationTarget {
            id: 1,
            public_id: "target-1".to_owned(),
            name: "WeCom".to_owned(),
            target_type: "wecom".to_owned(),
            config: json!({
                "webhookUrl": "http://127.0.0.1:9000/wecom"
            }),
        };

        let request = build_delivery_request(&target, &payload()).unwrap();

        assert_eq!(request.url, "http://127.0.0.1:9000/wecom");
        assert_eq!(request.body["msgtype"], "text");
        assert!(
            request.body["text"]["content"]
                .as_str()
                .unwrap()
                .contains("2 new items")
        );
    }

    #[test]
    fn webhook_headers_reject_sensitive_or_invalid_values() {
        let target = NotificationTarget {
            id: 1,
            public_id: "target-1".to_owned(),
            name: "Webhook".to_owned(),
            target_type: "webhook".to_owned(),
            config: json!({
                "url": "http://127.0.0.1:9000/notify",
                "headers": {
                    "host": "example.test"
                }
            }),
        };

        assert!(build_delivery_request(&target, &payload()).is_err());
    }

    #[test]
    fn retry_delay_is_bounded_exponential_backoff() {
        assert_eq!(retry_delay_seconds(1), 5);
        assert_eq!(retry_delay_seconds(2), 10);
        assert_eq!(retry_delay_seconds(6), 160);
        assert_eq!(retry_delay_seconds(12), 160);
    }

    #[test]
    fn notification_delivery_retry_logs_structured_event_context() {
        let production_source = include_str!("delivery.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("delivery source should include production section");

        assert!(production_source.contains("outbox_event_id = %event.public_id"));
        assert!(production_source.contains("attempt = event.attempt"));
        assert!(production_source.contains("max_attempts = event.max_attempts"));
        assert!(production_source.contains("retry_delay_seconds"));
        assert!(production_source.contains("notification delivery failed; scheduled retry"));
    }

    #[test]
    fn truncation_preserves_utf8_boundary() {
        assert_eq!(truncate_str("通知通知通知", 7), "通知");
    }

    fn request_body_is_complete(buffer: &[u8]) -> bool {
        let request = String::from_utf8_lossy(buffer);
        let Some(header_end) = request.find("\r\n\r\n") else {
            return false;
        };
        let headers = &request[..header_end];
        let content_length = headers.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        });
        let body_start = header_end + 4;
        let body_len = buffer.len().saturating_sub(body_start);
        body_len >= content_length.unwrap_or(0)
    }
}
