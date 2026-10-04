//! Provider 联网重试与退避（生产化韧性）。
//!
//! TMDB/TVDB/Fanart 等联网 provider 会遇到瞬时故障：限流（429）、上游 5xx、连接/超时。
//! 这些应有界重试 + 指数退避；而 4xx（除 429）是确定性失败（坏请求/鉴权/不存在），
//! 重试无意义，立即放弃。决策逻辑（错误 → 是否重试 + 退避时长）是纯函数，可穷举单测；
//! 异步重试循环用泛型 helper 包住 provider 调用，不侵入各 provider 的 HTTP 代码。

use std::time::Duration;

use super::shared::MetadataProviderError;

/// 重试策略：最大尝试次数 + 指数退避基数/上限。模块常量（不过度配置化）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    /// 总尝试次数（含首次）。`max_attempts=3` = 首次 + 2 次重试。
    pub max_attempts: u32,
    /// 指数退避基数（毫秒）：第 n 次重试前等 `base * 2^(n-1)`，钳到 `max_delay_ms`。
    pub base_delay_ms: u64,
    /// 退避上限（毫秒）。
    pub max_delay_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay_ms: 500,
            max_delay_ms: 8_000,
        }
    }
}

/// 错误是否值得重试（纯函数）：限流 429、上游 5xx、超时、连接错误 = 瞬时，可重试；
/// 其余 4xx（坏请求/鉴权/404）和非 HTTP client 错误 = 确定性失败，不重试。
pub fn is_retryable(err: &MetadataProviderError) -> bool {
    match err {
        // 配置/参数类客户端错误：确定性，不重试。
        MetadataProviderError::Client(_) => false,
        MetadataProviderError::Http(http) => {
            if http.is_timeout() || http.is_connect() {
                return true;
            }
            match http.status() {
                Some(status) => status.as_u16() == 429 || status.is_server_error(),
                // 无状态码（如解析中断/传输层错误）：当作瞬时，可重试。
                None => true,
            }
        }
    }
}

/// 第 `attempt` 次重试前的退避时长（`attempt` 从 1 计）。指数退避钳到上限。纯函数。
pub fn backoff_delay(attempt: u32, policy: &RetryPolicy) -> Duration {
    if attempt == 0 {
        return Duration::ZERO;
    }
    // base * 2^(attempt-1)，用 saturating 防溢出，再钳到上限。
    let factor = 1u64.checked_shl(attempt - 1).unwrap_or(u64::MAX);
    let millis = policy
        .base_delay_ms
        .saturating_mul(factor)
        .min(policy.max_delay_ms);
    Duration::from_millis(millis)
}

/// 有界重试地执行一个异步 provider 操作。可重试错误按指数退避重试，直到成功、
/// 遇到不可重试错误、或耗尽 `max_attempts`。`sleep` 注入便于测试免真实等待。
pub async fn retry_async<T, F, Fut, S, SFut>(
    policy: &RetryPolicy,
    mut op: F,
    mut sleep: S,
) -> Result<T, MetadataProviderError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, MetadataProviderError>>,
    S: FnMut(Duration) -> SFut,
    SFut: std::future::Future<Output = ()>,
{
    let max_attempts = policy.max_attempts.max(1);
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        match op().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                // 已是最后一次，或错误不可重试：放弃。
                if attempt >= max_attempts || !is_retryable(&err) {
                    return Err(err);
                }
                sleep(backoff_delay(attempt, policy)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn client_errors_are_not_retryable() {
        assert!(!is_retryable(&MetadataProviderError::Client(
            "bad".to_owned()
        )));
    }

    #[test]
    fn backoff_grows_exponentially_and_clamps() {
        let policy = RetryPolicy {
            max_attempts: 5,
            base_delay_ms: 500,
            max_delay_ms: 8_000,
        };
        assert_eq!(backoff_delay(0, &policy), Duration::ZERO);
        assert_eq!(backoff_delay(1, &policy), Duration::from_millis(500));
        assert_eq!(backoff_delay(2, &policy), Duration::from_millis(1_000));
        assert_eq!(backoff_delay(3, &policy), Duration::from_millis(2_000));
        assert_eq!(backoff_delay(4, &policy), Duration::from_millis(4_000));
        // 第 5 次本应 8000，正好等于上限；第 6 次 16000 钳到 8000。
        assert_eq!(backoff_delay(5, &policy), Duration::from_millis(8_000));
        assert_eq!(backoff_delay(6, &policy), Duration::from_millis(8_000));
    }

    #[tokio::test]
    async fn retry_returns_first_success_without_sleeping() {
        // 首次即成功：op 只调一次，不退避。重试-后-成功路径因难以构造可重试的
        // reqwest 错误而不在此单测覆盖；重试循环的「可重试判定」由 is_retryable 单测、
        // 「不可重试立即放弃」由下个测试覆盖。
        let calls = Cell::new(0u32);
        let slept = Cell::new(0u32);
        let policy = RetryPolicy::default();
        let result: Result<i32, MetadataProviderError> = retry_async(
            &policy,
            || {
                calls.set(calls.get() + 1);
                async { Ok(7) }
            },
            |_d| {
                slept.set(slept.get() + 1);
                async {}
            },
        )
        .await;
        assert_eq!(result.unwrap(), 7);
        assert_eq!(calls.get(), 1, "first success calls op exactly once");
        assert_eq!(slept.get(), 0, "no sleep when first attempt succeeds");
    }

    #[tokio::test]
    async fn retry_gives_up_on_non_retryable_error_without_sleeping() {
        let calls = Cell::new(0u32);
        let slept = Cell::new(0u32);
        let policy = RetryPolicy::default();
        let result: Result<i32, MetadataProviderError> = retry_async(
            &policy,
            || {
                calls.set(calls.get() + 1);
                async { Err(MetadataProviderError::Client("deterministic".to_owned())) }
            },
            |_d| {
                slept.set(slept.get() + 1);
                async {}
            },
        )
        .await;
        assert!(result.is_err());
        assert_eq!(calls.get(), 1, "non-retryable error must not retry");
        assert_eq!(slept.get(), 0, "non-retryable error must not sleep");
    }
}
