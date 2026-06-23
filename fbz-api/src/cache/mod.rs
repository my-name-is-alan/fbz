use std::{future::Future, time::Duration};

use redis::{Client, aio::ConnectionManager};

use crate::config::RedisConfig;

pub type RedisConnection = ConnectionManager;

pub async fn connect(config: &RedisConfig) -> redis::RedisResult<RedisConnection> {
    Client::open(config.url.as_str())?
        .get_connection_manager()
        .await
}

pub async fn ping(connection: &mut RedisConnection, timeout_ms: u64) -> redis::RedisResult<String> {
    with_operation_timeout(timeout_ms, redis::cmd("PING").query_async(connection)).await
}

pub async fn with_operation_timeout<T>(
    timeout_ms: u64,
    operation: impl Future<Output = redis::RedisResult<T>>,
) -> redis::RedisResult<T> {
    match tokio::time::timeout(Duration::from_millis(timeout_ms), operation).await {
        Ok(result) => result,
        Err(_) => Err(operation_timeout_error(timeout_ms)),
    }
}

fn operation_timeout_error(timeout_ms: u64) -> redis::RedisError {
    redis::RedisError::from((
        redis::ErrorKind::IoError,
        "redis operation timed out",
        format!("operation exceeded {timeout_ms}ms"),
    ))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test]
    async fn with_operation_timeout_returns_timeout_error() {
        let result = with_operation_timeout(1, async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok::<(), redis::RedisError>(())
        })
        .await;

        let err = result.expect_err("operation should time out");

        assert_eq!(err.kind(), redis::ErrorKind::IoError);
        assert!(err.to_string().contains("redis operation timed out"));
    }
}
