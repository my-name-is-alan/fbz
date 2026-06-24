use crate::{
    cache::{RedisConnection, with_operation_timeout},
    config::RedisConfig,
    events::repository::ClaimedOutboxEvent,
};

pub async fn publish_outbox_event(
    connection: &mut RedisConnection,
    config: &RedisConfig,
    event: &ClaimedOutboxEvent,
) -> redis::RedisResult<String> {
    let fields = stream_fields_for_event(event);
    let mut command = redis::cmd("XADD");
    command
        .arg(&config.event_stream_key)
        .arg("MAXLEN")
        .arg("~")
        .arg(config.event_stream_max_len)
        .arg("*");

    for (field, value) in fields {
        command.arg(field).arg(value);
    }

    with_operation_timeout(config.operation_timeout_ms, command.query_async(connection)).await
}

pub fn stream_fields_for_event(event: &ClaimedOutboxEvent) -> Vec<(&'static str, String)> {
    vec![
        ("outboxId", event.id.to_string()),
        ("eventId", event.public_id.clone()),
        ("eventType", event.event_type.clone()),
        ("aggregateType", event.aggregate_type.clone()),
        ("aggregateId", event.aggregate_id.clone()),
        ("payload", encode_payload(event)),
        ("status", event.status.clone()),
        ("attempts", event.attempts.to_string()),
        ("maxAttempts", event.max_attempts.to_string()),
        (
            "streamMirrorAttempts",
            event.stream_mirror_attempts.to_string(),
        ),
        ("availableAt", event.available_at.clone()),
        ("createdAt", event.created_at.clone()),
    ]
}

fn encode_payload(event: &ClaimedOutboxEvent) -> String {
    serde_json::to_string(&event.payload).unwrap_or_else(|_| "{}".to_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn stream_fields_expose_stable_public_event_boundary() {
        let event = ClaimedOutboxEvent {
            id: 42,
            public_id: "event-public-id".to_owned(),
            event_type: "plugin.hook.dispatch".to_owned(),
            aggregate_type: "plugin".to_owned(),
            aggregate_id: "tg-notifier".to_owned(),
            payload: json!({"hook": "user.login"}),
            status: "pending".to_owned(),
            attempts: 0,
            max_attempts: 10,
            available_at: "2026-06-19 00:00:00+08".to_owned(),
            created_at: "2026-06-19 00:00:00+08".to_owned(),
            stream_mirror_attempts: 1,
            stale_mirror_lease: false,
        };

        let fields = stream_fields_for_event(&event);

        assert_eq!(
            fields,
            vec![
                ("outboxId", "42".to_owned()),
                ("eventId", "event-public-id".to_owned()),
                ("eventType", "plugin.hook.dispatch".to_owned()),
                ("aggregateType", "plugin".to_owned()),
                ("aggregateId", "tg-notifier".to_owned()),
                ("payload", r#"{"hook":"user.login"}"#.to_owned()),
                ("status", "pending".to_owned()),
                ("attempts", "0".to_owned()),
                ("maxAttempts", "10".to_owned()),
                ("streamMirrorAttempts", "1".to_owned()),
                ("availableAt", "2026-06-19 00:00:00+08".to_owned()),
                ("createdAt", "2026-06-19 00:00:00+08".to_owned()),
            ]
        );
    }
}
