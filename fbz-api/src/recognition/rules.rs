//! 自定义识别词引擎（design §7）——净室重写，对齐 MoviePilot 的交互形态与规则语义。
//!
//! 四类规则（design §7.1）：
//! - 屏蔽词 `block`：从待识别串删除片段（噪音/广告标签）。
//! - 替换词 `replace`：`<被替换> => <替换为>`（纠正错误命名、统一别名）。
//! - 集数偏移 `offset`：`<前定位> <> <后定位> >> <偏移表达式>`（绝对集号 ↔ 季内集号）。
//! - 替换+偏移 `replace_offset`：`<替换段> && <偏移段>`。
//!
//! 引擎在识别管线阶段 A（block/replace 预处理）与阶段 D（offset 应用于已提取集号）介入。
//! 规则编译失败（坏正则）不阻断扫描——坏规则跳过并记 warn（design §3 不变量）。
//!
//! 净室：依据公开行为语义独立实现，分隔符 `=>` `<>` `>>` `&&` 是语义约定，非抄代码。

use regex::Regex;

/// 规则种类（对应 DB `recognition_words.kind`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleKind {
    Block,
    Replace,
    Offset,
    ReplaceOffset,
}

impl RuleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RuleKind::Block => "block",
            RuleKind::Replace => "replace",
            RuleKind::Offset => "offset",
            RuleKind::ReplaceOffset => "replace_offset",
        }
    }

    pub fn parse(value: &str) -> Option<RuleKind> {
        match value {
            "block" => Some(RuleKind::Block),
            "replace" => Some(RuleKind::Replace),
            "offset" => Some(RuleKind::Offset),
            "replace_offset" => Some(RuleKind::ReplaceOffset),
            _ => None,
        }
    }
}

/// 一条结构化识别词规则（admin 录入解析后的形态，对应 DB 行）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecognitionWord {
    pub id: String,
    pub kind: RuleKind,
    /// 左件：屏蔽词 / 被替换词 / 前定位词。
    pub pattern: String,
    /// 右件：替换为（offset 类为 None）。
    pub replacement: Option<String>,
    /// offset 类：后定位词。
    pub anchor_after: Option<String>,
    /// offset 类：集数偏移表达式，如 `-26` 或 `EP*2-1`。
    pub offset_expr: Option<String>,
    /// 左件/定位词是否按正则解释（否则字面，自动转义）。
    pub is_regex: bool,
    /// 应用顺序，小者先。
    pub priority: i32,
}

/// 录入语法解析错误（admin API 回报）。
#[derive(Debug, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    MissingReplacement,
    MissingOffsetExpr,
    BadOffsetExpr(String),
    BadRegex(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::Empty => write!(f, "rule line is empty"),
            ParseError::MissingReplacement => write!(f, "replace rule requires '=> replacement'"),
            ParseError::MissingOffsetExpr => write!(f, "offset rule requires '>> expression'"),
            ParseError::BadOffsetExpr(e) => write!(f, "invalid offset expression: {e}"),
            ParseError::BadRegex(e) => write!(f, "invalid regex: {e}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// 解析一行录入文本为结构化字段（design §7.1 语法）。`is_regex` 由调用方另行指定。
/// 分隔符优先级：先按 `&&` 拆替换段/偏移段，替换段按 `=>`，偏移段按 `<>` / `>>`。
pub fn parse_rule_line(line: &str, is_regex: bool) -> Result<ParsedRule, ParseError> {
    let line = line.trim();
    if line.is_empty() {
        return Err(ParseError::Empty);
    }

    // replace_offset：`<替换段> && <偏移段>`。
    if let Some((replace_part, offset_part)) = line.split_once("&&") {
        let (pattern, replacement) = parse_replace_part(replace_part)?;
        let (anchor_before, anchor_after, offset_expr) = parse_offset_part(offset_part)?;
        validate_offset_expr(&offset_expr)?;
        return Ok(ParsedRule {
            kind: RuleKind::ReplaceOffset,
            pattern: pattern.unwrap_or(anchor_before),
            replacement: Some(replacement),
            anchor_after: Some(anchor_after),
            offset_expr: Some(offset_expr),
            is_regex,
        });
    }

    // offset：`<前定位> <> <后定位> >> <偏移表达式>`。
    if line.contains(">>") || line.contains("<>") {
        let (anchor_before, anchor_after, offset_expr) = parse_offset_part(line)?;
        validate_offset_expr(&offset_expr)?;
        return Ok(ParsedRule {
            kind: RuleKind::Offset,
            pattern: anchor_before,
            replacement: None,
            anchor_after: Some(anchor_after),
            offset_expr: Some(offset_expr),
            is_regex,
        });
    }

    // replace：`<被替换> => <替换为>`。
    if line.contains("=>") {
        let (pattern, replacement) = parse_replace_part(line)?;
        return Ok(ParsedRule {
            kind: RuleKind::Replace,
            pattern: pattern.ok_or(ParseError::Empty)?,
            replacement: Some(replacement),
            anchor_after: None,
            offset_expr: None,
            is_regex,
        });
    }

    // block：整行就是屏蔽词。
    Ok(ParsedRule {
        kind: RuleKind::Block,
        pattern: line.to_owned(),
        replacement: None,
        anchor_after: None,
        offset_expr: None,
        is_regex,
    })
}

/// 录入解析结果（admin API 拆出结构化列写库用）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedRule {
    pub kind: RuleKind,
    pub pattern: String,
    pub replacement: Option<String>,
    pub anchor_after: Option<String>,
    pub offset_expr: Option<String>,
    pub is_regex: bool,
}

fn parse_replace_part(part: &str) -> Result<(Option<String>, String), ParseError> {
    let (left, right) = part
        .split_once("=>")
        .ok_or(ParseError::MissingReplacement)?;
    let left = left.trim();
    let right = right.trim().to_owned();
    Ok(((!left.is_empty()).then(|| left.to_owned()), right))
}

/// 偏移段：`<前定位> <> <后定位> >> <表达式>`。定位词可空（无窗口约束）。
fn parse_offset_part(part: &str) -> Result<(String, String, String), ParseError> {
    let (anchors, expr) = part.split_once(">>").ok_or(ParseError::MissingOffsetExpr)?;
    let expr = expr.trim().to_owned();
    let (before, after) = match anchors.split_once("<>") {
        Some((b, a)) => (b.trim().to_owned(), a.trim().to_owned()),
        None => (anchors.trim().to_owned(), String::new()),
    };
    Ok((before, after, expr))
}

// ---- 偏移表达式求值（design §7.2，自写极简解析器，仅 + - * 与整数、EP）----

/// 求值偏移表达式：`EP` 代入原集号 `ep`。支持 `+ - *` 与整数、`EP`，无括号。
/// 越界（结果 ≤ 0）返回 None，调用方丢弃该偏移保留原集号（design §7.3）。
///
/// 两种语义（design §7.2）：
/// - **常量偏移**（不含 EP，如 `-26`/`+12`）：隐含 `EP + 表达式`，即在原集号上加偏移量。
/// - **线性表达式**（含 EP，如 `EP-26`/`EP*2-1`）：直接求值为新集号。
pub fn eval_offset_expr(expr: &str, ep: i32) -> Option<i32> {
    let tokens = tokenize_expr(expr)?;
    let has_ep = tokens.iter().any(|t| matches!(t, ExprToken::Ep));
    let raw = eval_tokens(&tokens, ep)?;
    let value = if has_ep { raw } else { ep.checked_add(raw)? };
    (value > 0).then_some(value)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExprToken {
    Num(i32),
    Ep,
    Plus,
    Minus,
    Star,
}

fn tokenize_expr(expr: &str) -> Option<Vec<ExprToken>> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = expr.chars().filter(|c| !c.is_whitespace()).collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '+' => {
                tokens.push(ExprToken::Plus);
                i += 1;
            }
            '-' => {
                tokens.push(ExprToken::Minus);
                i += 1;
            }
            '*' => {
                tokens.push(ExprToken::Star);
                i += 1;
            }
            'E' | 'e' => {
                // EP（不区分大小写）。
                if i + 1 < chars.len() && matches!(chars[i + 1], 'P' | 'p') {
                    tokens.push(ExprToken::Ep);
                    i += 2;
                } else {
                    return None;
                }
            }
            d if d.is_ascii_digit() => {
                let mut num = 0i32;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    num = num
                        .checked_mul(10)?
                        .checked_add((chars[i] as u8 - b'0') as i32)?;
                    i += 1;
                }
                tokens.push(ExprToken::Num(num));
            }
            _ => return None,
        }
    }
    (!tokens.is_empty()).then_some(tokens)
}

/// 求值：先乘后加减，左结合。一元正负由前导 +/- 处理。无括号，极简两遍扫描。
fn eval_tokens(tokens: &[ExprToken], ep: i32) -> Option<i32> {
    // 第一遍：把 Num/Ep 解析为值，处理 * （高优先级）。
    // 用 (值, 后续运算符) 序列，先折叠所有 *，再折叠 +/-。
    #[derive(Clone, Copy)]
    enum Op {
        Add,
        Sub,
        Mul,
    }

    // 解析为 [value] 与 [op] 交替；支持前导 - / +（一元）。
    let mut values: Vec<i32> = Vec::new();
    let mut ops: Vec<Op> = Vec::new();
    let mut expect_value = true;
    let mut unary_neg = false;
    for &tok in tokens {
        match tok {
            ExprToken::Num(n) if expect_value => {
                values.push(if unary_neg { -n } else { n });
                expect_value = false;
                unary_neg = false;
            }
            ExprToken::Ep if expect_value => {
                values.push(if unary_neg { -ep } else { ep });
                expect_value = false;
                unary_neg = false;
            }
            ExprToken::Minus if expect_value => {
                unary_neg = !unary_neg;
            }
            ExprToken::Plus if expect_value => { /* 一元正号忽略 */ }
            ExprToken::Plus if !expect_value => {
                ops.push(Op::Add);
                expect_value = true;
            }
            ExprToken::Minus if !expect_value => {
                ops.push(Op::Sub);
                expect_value = true;
            }
            ExprToken::Star if !expect_value => {
                ops.push(Op::Mul);
                expect_value = true;
            }
            _ => return None,
        }
    }
    if expect_value || values.len() != ops.len() + 1 {
        return None;
    }

    // 折叠 * （从左到右）。
    let mut v2: Vec<i32> = vec![values[0]];
    let mut o2: Vec<Op> = Vec::new();
    for (idx, op) in ops.iter().enumerate() {
        match op {
            Op::Mul => {
                let last = v2.last_mut()?;
                *last = last.checked_mul(values[idx + 1])?;
            }
            other => {
                o2.push(*other);
                v2.push(values[idx + 1]);
            }
        }
    }

    // 折叠 +/-。
    let mut acc = v2[0];
    for (idx, op) in o2.iter().enumerate() {
        match op {
            Op::Add => acc = acc.checked_add(v2[idx + 1])?,
            Op::Sub => acc = acc.checked_sub(v2[idx + 1])?,
            Op::Mul => unreachable!("mul already folded"),
        }
    }
    Some(acc)
}

/// 校验偏移表达式可解析（admin 录入时调用，坏表达式回报错误）。
fn validate_offset_expr(expr: &str) -> Result<(), ParseError> {
    // 用 ep=1 试求值（仅验证语法，不验证越界——越界是运行时数据相关）。
    if tokenize_expr(expr)
        .and_then(|t| eval_tokens(&t, 1))
        .is_none()
    {
        return Err(ParseError::BadOffsetExpr(expr.to_owned()));
    }
    Ok(())
}

// ---- RuleSet 编译与应用 ----

/// 编译后的单条规则（正则已编译；字面词转义为正则）。
struct CompiledRule {
    id: String,
    kind: RuleKind,
    matcher: Regex,
    replacement: Option<String>,
    offset_expr: Option<String>,
    #[allow(dead_code)]
    priority: i32,
}

/// 编译后的规则集（内存形态，识别管线持有）。
pub struct RuleSet {
    rules: Vec<CompiledRule>,
}

impl RuleSet {
    /// 从 DB 行编译。坏正则跳过并记入 `skipped`（design §3：不阻断扫描）。
    /// 规则按 priority 升序应用。
    pub fn compile(words: Vec<RecognitionWord>) -> (RuleSet, Vec<String>) {
        let mut rules = Vec::new();
        let mut skipped = Vec::new();
        let mut sorted = words;
        sorted.sort_by_key(|w| (w.priority, w.id.clone()));
        for word in sorted {
            let pattern = if word.is_regex {
                word.pattern.clone()
            } else {
                regex::escape(&word.pattern)
            };
            match Regex::new(&pattern) {
                Ok(matcher) => rules.push(CompiledRule {
                    id: word.id,
                    kind: word.kind,
                    matcher,
                    replacement: word.replacement,
                    offset_expr: word.offset_expr,
                    priority: word.priority,
                }),
                Err(err) => skipped.push(format!("{}: {err}", word.id)),
            }
        }
        (RuleSet { rules }, skipped)
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// 阶段 A：对待识别串施加 block + replace 规则。返回处理后的串 + 命中规则 id。
    /// 幂等：每条规则对该串只作用一次（replace 不递归重扫，避免 A→B→A 循环，design §7.3）。
    pub fn apply_preprocess(&self, input: &str) -> (String, Vec<String>) {
        let mut text = input.to_owned();
        let mut matched = Vec::new();
        for rule in &self.rules {
            match rule.kind {
                RuleKind::Block => {
                    if rule.matcher.is_match(&text) {
                        text = rule.matcher.replace_all(&text, "").into_owned();
                        matched.push(rule.id.clone());
                    }
                }
                RuleKind::Replace | RuleKind::ReplaceOffset => {
                    if let Some(replacement) = &rule.replacement
                        && rule.matcher.is_match(&text)
                    {
                        // replace_n(.., 0, ..) 不做，这里只替换首个匹配位置集合一次。
                        text = rule
                            .matcher
                            .replace(&text, replacement.as_str())
                            .into_owned();
                        matched.push(rule.id.clone());
                    }
                }
                RuleKind::Offset => {}
            }
        }
        (
            text.split_whitespace().collect::<Vec<_>>().join(" "),
            matched,
        )
    }

    /// 阶段 D：对已提取集号施加 offset 规则。`context` 是原始串（用于定位词窗口匹配）。
    /// 定位词为空表示无窗口约束（整串生效）。越界结果丢弃保留原集号（design §7.3）。
    pub fn apply_offset(&self, context: &str, episode: i32) -> (i32, Vec<String>) {
        let mut ep = episode;
        let mut matched = Vec::new();
        for rule in &self.rules {
            if !matches!(rule.kind, RuleKind::Offset | RuleKind::ReplaceOffset) {
                continue;
            }
            let Some(expr) = &rule.offset_expr else {
                continue;
            };
            // 前定位词（matcher）非空时要求命中 context；空 matcher（escape("")）匹配任意位置。
            if rule.matcher.as_str().is_empty() || rule.matcher.is_match(context) {
                if let Some(new_ep) = eval_offset_expr(expr, ep) {
                    ep = new_ep;
                    matched.push(rule.id.clone());
                }
                // 越界（None）：丢弃，保留原 ep，不记命中。
            }
        }
        (ep, matched)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- 录入语法解析（§7.1） ----

    #[test]
    fn parses_block_rule() {
        let r = parse_rule_line("[广告组]", false).unwrap();
        assert_eq!(r.kind, RuleKind::Block);
        assert_eq!(r.pattern, "[广告组]");
    }

    #[test]
    fn parses_replace_rule() {
        let r = parse_rule_line("斗破苍穹年番 => 斗破苍穹", false).unwrap();
        assert_eq!(r.kind, RuleKind::Replace);
        assert_eq!(r.pattern, "斗破苍穹年番");
        assert_eq!(r.replacement.as_deref(), Some("斗破苍穹"));
    }

    #[test]
    fn parses_offset_rule() {
        let r = parse_rule_line("SP <> 结束 >> EP-12", false).unwrap();
        assert_eq!(r.kind, RuleKind::Offset);
        assert_eq!(r.pattern, "SP");
        assert_eq!(r.anchor_after.as_deref(), Some("结束"));
        assert_eq!(r.offset_expr.as_deref(), Some("EP-12"));
    }

    #[test]
    fn parses_replace_offset_rule() {
        let r = parse_rule_line("名字A => 名字B && 前 <> 后 >> EP-26", false).unwrap();
        assert_eq!(r.kind, RuleKind::ReplaceOffset);
        assert_eq!(r.replacement.as_deref(), Some("名字B"));
        assert_eq!(r.offset_expr.as_deref(), Some("EP-26"));
    }

    #[test]
    fn rejects_bad_offset_expr() {
        let err = parse_rule_line("A <> B >> EP**", false).unwrap_err();
        assert!(matches!(err, ParseError::BadOffsetExpr(_)));
    }

    #[test]
    fn rejects_empty_line() {
        assert_eq!(
            parse_rule_line("   ", false).unwrap_err(),
            ParseError::Empty
        );
    }

    // ---- 偏移表达式求值（§7.2） ----

    #[test]
    fn eval_constant_offset() {
        assert_eq!(eval_offset_expr("-26", 38), Some(12));
        assert_eq!(eval_offset_expr("+12", 1), Some(13));
    }

    #[test]
    fn eval_linear_offset() {
        assert_eq!(eval_offset_expr("EP-26", 38), Some(12));
        assert_eq!(eval_offset_expr("EP*2-1", 5), Some(9));
        assert_eq!(eval_offset_expr("EP*2", 6), Some(12));
    }

    #[test]
    fn eval_offset_out_of_bounds_returns_none() {
        // 算出 ≤ 0 → None（调用方保留原集号）。
        assert_eq!(eval_offset_expr("EP-26", 10), None);
        assert_eq!(eval_offset_expr("EP-5", 5), None);
    }

    #[test]
    fn eval_rejects_garbage() {
        assert_eq!(eval_offset_expr("EP**2", 5), None);
        assert_eq!(eval_offset_expr("abc", 5), None);
        assert_eq!(eval_offset_expr("", 5), None);
    }

    // ---- RuleSet 编译与应用（§7.3） ----

    fn word(
        id: &str,
        kind: RuleKind,
        pattern: &str,
        repl: Option<&str>,
        expr: Option<&str>,
        prio: i32,
    ) -> RecognitionWord {
        RecognitionWord {
            id: id.to_owned(),
            kind,
            pattern: pattern.to_owned(),
            replacement: repl.map(str::to_owned),
            anchor_after: None,
            offset_expr: expr.map(str::to_owned),
            is_regex: false,
            priority: prio,
        }
    }

    #[test]
    fn ruleset_applies_block_then_replace_by_priority() {
        let (rs, skipped) = RuleSet::compile(vec![
            word("r1", RuleKind::Block, "[噪音]", None, None, 10),
            word("r2", RuleKind::Replace, "别名", Some("正名"), None, 20),
        ]);
        assert!(skipped.is_empty());
        let (out, matched) = rs.apply_preprocess("别名 [噪音] S01E01");
        assert!(out.contains("正名"));
        assert!(!out.contains("噪音"));
        assert_eq!(matched.len(), 2);
    }

    #[test]
    fn ruleset_bad_regex_is_skipped_not_fatal() {
        let (rs, skipped) = RuleSet::compile(vec![
            word("bad", RuleKind::Block, "[unclosed", None, None, 10).with_regex(),
        ]);
        // 坏正则跳过，RuleSet 仍可用（空）。
        assert_eq!(skipped.len(), 1);
        assert!(rs.is_empty());
    }

    #[test]
    fn ruleset_offset_applies_to_episode() {
        let (rs, _) = RuleSet::compile(vec![word(
            "off",
            RuleKind::Offset,
            "", // 无前定位窗口约束
            None,
            Some("EP-26"),
            10,
        )]);
        let (ep, matched) = rs.apply_offset("Show - 38", 38);
        assert_eq!(ep, 12);
        assert_eq!(matched, vec!["off".to_owned()]);
    }

    #[test]
    fn ruleset_offset_out_of_bounds_preserves_episode() {
        let (rs, _) = RuleSet::compile(vec![word(
            "off",
            RuleKind::Offset,
            "",
            None,
            Some("EP-100"),
            10,
        )]);
        let (ep, matched) = rs.apply_offset("x", 5);
        assert_eq!(ep, 5, "out-of-bounds offset must preserve original episode");
        assert!(matched.is_empty());
    }

    impl RecognitionWord {
        fn with_regex(mut self) -> Self {
            self.is_regex = true;
            self
        }
    }
}
