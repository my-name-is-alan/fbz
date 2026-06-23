use redis::{Client, aio::ConnectionManager};

use crate::config::RedisConfig;

pub type RedisConnection = ConnectionManager;

pub async fn connect(config: &RedisConfig) -> redis::RedisResult<RedisConnection> {
    Client::open(config.url.as_str())?
        .get_connection_manager()
        .await
}

pub async fn ping(connection: &mut RedisConnection) -> redis::RedisResult<String> {
    redis::cmd("PING").query_async(connection).await
}
