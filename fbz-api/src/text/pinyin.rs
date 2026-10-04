//! 中文名拼音拆分：为含中文的名称生成「全拼」与「首字母」检索键，支撑模糊查询。
//!
//! 入库时（演员 / 影视 / 图片 / 音乐）凡含中文的名称都过一遍 [`pinyin_keys`]，把结果落库到
//! `pinyin_full` / `pinyin_initials` 列；查询时把用户输入同样归一后对这两列做 `like` 匹配，
//! 即可用「quanpin」或「qp」首字母命中中文条目。
//!
//! 设计取舍：
//! - 多音字取 `pinyin` crate 的首选读音（不做分词消歧）——检索场景够用，避免引入分词依赖。
//! - 非中文字符（英文 / 数字）原样保留并入键，使「刘 Tom」这类混名两段都可命中。
//! - 名称不含任何中文时返回 `None`：纯英文名无需额外拼音键，省存储且避免污染匹配。

use pinyin::ToPinyin;

/// 一个名称的拼音检索键。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PinyinKeys {
    /// 全拼（小写、无分隔），如「刘德华」→ `liudehua`；混名「刘 Tom」→ `liutom`。
    pub full: String,
    /// 首字母（小写），如「刘德华」→ `ldh`；混名「刘 Tom」→ `lt`。
    pub initials: String,
}

/// 为名称生成拼音检索键；名称不含中文时返回 `None`。
///
/// 中文逐字转拼音（多音字取首选读音）；ASCII 字母 / 数字原样并入（全拼收全部、首字母收每个
/// 连续 ASCII 段的首字符）；其余字符（空格 / 标点）作分隔，不进键。
pub fn pinyin_keys(name: &str) -> Option<PinyinKeys> {
    let mut full = String::new();
    let mut initials = String::new();
    let mut saw_chinese = false;
    // 标记上一字符是否为 ASCII 字母数字，用来切分「连续 ASCII 段」取其首字母。
    let mut in_ascii_run = false;

    for ch in name.chars() {
        if let Some(syllable) = ch.to_pinyin() {
            // 命中中文：取首选读音的全拼 + 声母首字母。
            saw_chinese = true;
            in_ascii_run = false;
            let plain = syllable.plain();
            full.push_str(plain);
            if let Some(first) = plain.chars().next() {
                initials.push(first);
            }
        } else if ch.is_ascii_alphanumeric() {
            // 非中文字母数字：原样并入全拼；每个连续 ASCII 段只取首字符进首字母。
            full.push(ch.to_ascii_lowercase());
            if !in_ascii_run {
                initials.push(ch.to_ascii_lowercase());
                in_ascii_run = true;
            }
        } else {
            // 空格 / 标点：作分隔，重置 ASCII 段。
            in_ascii_run = false;
        }
    }

    if !saw_chinese {
        return None;
    }

    Some(PinyinKeys { full, initials })
}

/// 归一化用户查询词用于拼音匹配：去空白、转小写、只留字母数字。
/// 与入库键的字符集一致，使「L D H」「ldh」「l-d-h」都能命中 `ldh`。
pub fn normalize_pinyin_query(term: &str) -> String {
    term.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_chinese_name_splits_full_and_initials() {
        let keys = pinyin_keys("刘德华").expect("should produce pinyin keys");
        assert_eq!(keys.full, "liudehua");
        assert_eq!(keys.initials, "ldh");
    }

    #[test]
    fn pure_ascii_name_returns_none() {
        assert_eq!(pinyin_keys("Tom Hanks"), None);
        assert_eq!(pinyin_keys("2049"), None);
    }

    #[test]
    fn mixed_name_keeps_both_segments() {
        let keys = pinyin_keys("刘 Tom").expect("mixed name should produce keys");
        assert_eq!(keys.full, "liutom");
        // 中文「刘」→ l；连续 ASCII 段「Tom」→ 段首字母 t。
        assert_eq!(keys.initials, "lt");
    }

    #[test]
    fn punctuation_separates_ascii_runs_for_initials() {
        // 「周杰伦 J-Lin」：中文 zhou jie lun → zjl，ASCII 段 j / lin → j l。
        let keys = pinyin_keys("周杰伦 J-Lin").expect("should produce keys");
        assert_eq!(keys.initials, "zjljl");
        assert_eq!(keys.full, "zhoujielunjlin");
    }

    #[test]
    fn query_normalization_strips_separators_and_case() {
        assert_eq!(normalize_pinyin_query("L D H"), "ldh");
        assert_eq!(normalize_pinyin_query("Liu-De-Hua"), "liudehua");
        assert_eq!(normalize_pinyin_query("  ldh  "), "ldh");
    }
}
