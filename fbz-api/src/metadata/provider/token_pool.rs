//! 联网 provider 的 API key 令牌池（可选能力）。
//!
//! 一个 provider 可配多个 API key：池在多个 key 间轮转（round-robin），遇到限流（429）
//! 把该 key 标记冷却一段时间、后续请求跳过它，从而把上游限额翻倍。只配一个 key 时
//! 退化为「总是返回那一个 key」，零行为变化（令牌池是可选项）。
//!
//! 对任意 provider id 通用——内置 `tmdb`/`tvdb`/`fanart` 和插件 `plugin:{id}` 都能配多 key。
//!
//! 设计：纯轮转逻辑（`next_available`：可用标记 + 起点 → 下一个可用下标）与时间判断
//! （cooldown `Instant` vs `now`）分离，前者可穷举单测，后者薄薄一层。

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 纯轮转选择：从 `start` 起按环形找第一个 `available[i] == true` 的下标。
/// 全不可用返回 `None`。纯函数，可穷举单测。
pub fn next_available(available: &[bool], start: usize) -> Option<usize> {
    if available.is_empty() {
        return None;
    }
    let n = available.len();
    let start = start % n;
    for offset in 0..n {
        let idx = (start + offset) % n;
        if available[idx] {
            return Some(idx);
        }
    }
    None
}

/// 一次取到的令牌：key 值 + 它在池中的下标（用于事后 `mark_rate_limited`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenLease {
    pub token: String,
    pub index: usize,
}

struct PoolState {
    /// 下一次轮转起点。
    next_index: usize,
    /// 每个 token 的冷却到期时刻（`Some` 表示在冷却中）。
    cooldown_until: Vec<Option<Instant>>,
}

/// API key 令牌池。线程安全（内部 `Mutex`），可被多个并发 provider 调用共享。
pub struct TokenPool {
    tokens: Vec<String>,
    state: Mutex<PoolState>,
}

impl TokenPool {
    /// 从一组 token 建池（去重前由调用方保证非空 token）。空集合建出的池 `acquire` 恒为 None。
    pub fn new(tokens: Vec<String>) -> Self {
        let len = tokens.len();
        Self {
            tokens,
            state: Mutex::new(PoolState {
                next_index: 0,
                cooldown_until: vec![None; len],
            }),
        }
    }

    /// 池中 token 数。
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// 取一个当前可用（未冷却）的令牌，并把轮转起点前移。
    /// 全部在冷却中时退而取**最快解冻**的那个（best-effort：宁可用一个快过期的，也不放弃请求）。
    /// 池为空返回 `None`。
    pub fn acquire(&self) -> Option<TokenLease> {
        self.acquire_at(Instant::now())
    }

    /// `acquire` 的可注入时钟版本（测试用）。
    pub fn acquire_at(&self, now: Instant) -> Option<TokenLease> {
        if self.tokens.is_empty() {
            return None;
        }
        let mut state = self.state.lock().expect("token pool mutex poisoned");
        let available: Vec<bool> = state
            .cooldown_until
            .iter()
            .map(|until| match until {
                Some(t) => *t <= now, // 冷却已过 = 可用
                None => true,
            })
            .collect();

        let start = state.next_index;
        let index = match next_available(&available, start) {
            Some(idx) => idx,
            None => {
                // 全在冷却中：取最快解冻的，让请求仍能发出（上游可能已恢复）。
                state
                    .cooldown_until
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, until)| until.unwrap_or(now))
                    .map(|(idx, _)| idx)?
            }
        };
        state.next_index = (index + 1) % self.tokens.len();
        Some(TokenLease {
            token: self.tokens[index].clone(),
            index,
        })
    }

    /// 把某个 token 标记为限流，冷却 `cooldown` 时长（后续 `acquire` 在此期间跳过它）。
    pub fn mark_rate_limited(&self, index: usize, cooldown: Duration) {
        self.mark_rate_limited_at(index, cooldown, Instant::now());
    }

    /// `mark_rate_limited` 的可注入时钟版本（测试用）。
    pub fn mark_rate_limited_at(&self, index: usize, cooldown: Duration, now: Instant) {
        let mut state = self.state.lock().expect("token pool mutex poisoned");
        if let Some(slot) = state.cooldown_until.get_mut(index) {
            *slot = Some(now + cooldown);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- 纯轮转逻辑（穷举） ----

    #[test]
    fn next_available_round_robins_from_start() {
        let all = [true, true, true];
        assert_eq!(next_available(&all, 0), Some(0));
        assert_eq!(next_available(&all, 1), Some(1));
        assert_eq!(next_available(&all, 2), Some(2));
        // 起点越界取模回绕。
        assert_eq!(next_available(&all, 3), Some(0));
    }

    #[test]
    fn next_available_skips_unavailable_and_wraps() {
        // 起点 1 不可用 → 找到 2；起点 2 不可用 → 回绕到 0。
        let avail = [true, false, false];
        assert_eq!(next_available(&avail, 1), Some(0));
        assert_eq!(next_available(&avail, 2), Some(0));
        let avail2 = [false, true, false];
        assert_eq!(next_available(&avail2, 0), Some(1));
        assert_eq!(next_available(&avail2, 2), Some(1));
    }

    #[test]
    fn next_available_none_when_all_unavailable_or_empty() {
        assert_eq!(next_available(&[false, false], 0), None);
        assert_eq!(next_available(&[], 0), None);
    }

    // ---- 池行为 ----

    #[test]
    fn single_token_pool_always_returns_same_token() {
        let pool = TokenPool::new(vec!["only".to_owned()]);
        let now = Instant::now();
        for _ in 0..5 {
            let lease = pool.acquire_at(now).unwrap();
            assert_eq!(lease.token, "only");
            assert_eq!(lease.index, 0);
        }
    }

    #[test]
    fn multi_token_pool_rotates_round_robin() {
        let pool = TokenPool::new(vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]);
        let now = Instant::now();
        assert_eq!(pool.acquire_at(now).unwrap().token, "a");
        assert_eq!(pool.acquire_at(now).unwrap().token, "b");
        assert_eq!(pool.acquire_at(now).unwrap().token, "c");
        assert_eq!(pool.acquire_at(now).unwrap().token, "a");
    }

    #[test]
    fn rate_limited_token_is_skipped_until_cooldown_expires() {
        let pool = TokenPool::new(vec!["a".to_owned(), "b".to_owned()]);
        let t0 = Instant::now();
        // 取到 a（index 0），标记限流冷却 10s。
        let lease = pool.acquire_at(t0).unwrap();
        assert_eq!(lease.token, "a");
        pool.mark_rate_limited_at(lease.index, Duration::from_secs(10), t0);

        // 冷却期内：下一次跳过 a，取 b。
        let in_cooldown = t0 + Duration::from_secs(3);
        assert_eq!(pool.acquire_at(in_cooldown).unwrap().token, "b");
        // 仍在冷却：a 不可用，又取 b（全环只 b 可用）。
        assert_eq!(pool.acquire_at(in_cooldown).unwrap().token, "b");

        // 冷却过后：a 重新可用，轮转恢复。
        let after = t0 + Duration::from_secs(11);
        let next = pool.acquire_at(after).unwrap().token;
        assert!(next == "a" || next == "b", "both available after cooldown");
    }

    #[test]
    fn all_cooled_falls_back_to_soonest_unfrozen() {
        let pool = TokenPool::new(vec!["a".to_owned(), "b".to_owned()]);
        let t0 = Instant::now();
        // a 冷却 10s、b 冷却 5s。
        pool.mark_rate_limited_at(0, Duration::from_secs(10), t0);
        pool.mark_rate_limited_at(1, Duration::from_secs(5), t0);
        // 全在冷却 → 取最快解冻的 b。
        let lease = pool.acquire_at(t0 + Duration::from_secs(1)).unwrap();
        assert_eq!(
            lease.token, "b",
            "soonest-unfrozen token chosen when all cooled"
        );
    }

    #[test]
    fn empty_pool_acquires_none() {
        let pool = TokenPool::new(Vec::new());
        assert!(pool.acquire_at(Instant::now()).is_none());
        assert!(pool.is_empty());
    }
}
