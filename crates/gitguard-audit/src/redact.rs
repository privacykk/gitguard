//! 审计与诊断输出前的统一脱敏。
//!
//! 顺序是「先识别秘密，再过滤控制字符，最后截断」：
//! - PEM 私钥整段替换，避免把多行密钥写进一行 JSONL；
//! - `KEY=VALUE` 形式的敏感变量只保留键名；
//! - 常见 Token 前缀（GitHub、Slack、OpenAI、AWS）整值替换；
//! - URL 的 userinfo 换成 `***`；
//! - 控制字符与 bidi 控制符换成空格，防止日志内容影响终端显示；
//! - 单字段最多 [`MAX_FIELD_CHARS`] 个字符，避免一条记录无限膨胀。
//!
//! 这些规则只做「少写」，不改变任何放行判断；漏掉某条规则的最坏结果是
//! 日志里多一段文本，而不是多一次放行。

/// 单个审计/诊断字段保留的最大字符数（按字符计，不是字节）。
pub const MAX_FIELD_CHARS: usize = 512;

/// 私钥整段替换后的占位文本。
const PRIVATE_KEY_PLACEHOLDER: &str = "[redacted private key]";

/// 命中 Token 前缀后的占位文本。
const TOKEN_PLACEHOLDER: &str = "***";

/// 键名里出现这些片段时，`KEY=VALUE` 的值一律替换。
///
/// 不使用单独的 `AUTH`，避免把 `AUTHOR=Ada` 这类普通字段也抹掉。
const SENSITIVE_KEY_PARTS: [&str; 11] = [
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "PASSWD",
    "PRIVATE_KEY",
    "CREDENTIAL",
    "API_KEY",
    "APIKEY",
    "ACCESS_KEY",
    "SESSION",
    "COOKIE",
];

/// 这些前缀本身就是凭据，命中后整个 token 都会被替换。
const CREDENTIAL_PREFIXES: [&str; 11] = [
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "xoxa-",
    "xoxb-",
    "xoxp-",
    "xoxr-",
    "xoxs-",
];

/// 前缀较短、容易撞上普通单词，只有后面还有足够长的随机串时才替换。
const SHORT_CREDENTIAL_PREFIXES: [&str; 2] = ["AKIA", "sk-"];

/// 过滤控制字符与 bidi 控制符，避免审计内容借终端转义序列干扰显示。
pub fn sanitize_text(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_control() || is_bidi_control(character) {
                ' '
            } else {
                character
            }
        })
        .collect()
}

/// 判断是否是改变显示顺序的双向文本控制符（RTL/LTR override 等）。
fn is_bidi_control(character: char) -> bool {
    matches!(
        character,
        '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
    )
}

/// 隐去 URL 里的用户名与密码。
///
/// `https://user:token@example.com/repo.git` → `https://***@example.com/repo.git`。
/// 只在 `scheme://` 的 authority 部分查找 `@`，路径里合法的 `@` 不会被误删；
/// 没有 userinfo 或不是 `scheme://` 形式（如 `git@example.com:repo.git`）时原样返回。
/// 输出同样会过滤控制字符。
pub fn redact_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return sanitize_text(url);
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(authority_end);
    let authority = match authority.rsplit_once('@') {
        Some((_userinfo, host)) => format!("***@{host}"),
        None => authority.to_string(),
    };
    format!("{}://{}{}", scheme, authority, redact_query(tail))
}

/// 隐去 query 里敏感参数的取值（如 `?access_token=...`），保留参数名便于排查。
fn redact_query(tail: &str) -> String {
    let (before_fragment, fragment) = match tail.split_once('#') {
        Some((head, fragment)) => (head, Some(fragment)),
        None => (tail, None),
    };
    let Some((path, query)) = before_fragment.split_once('?') else {
        return sanitize_text(tail);
    };

    let mut redacted = String::with_capacity(tail.len());
    redacted.push_str(path);
    redacted.push('?');
    for (index, pair) in query.split('&').enumerate() {
        if index > 0 {
            redacted.push('&');
        }
        match pair.split_once('=') {
            Some((key, _value)) if is_sensitive_key(key) => {
                redacted.push_str(key);
                redacted.push_str("=***");
            }
            _ => redacted.push_str(pair),
        }
    }
    if let Some(fragment) = fragment {
        redacted.push('#');
        redacted.push_str(fragment);
    }
    sanitize_text(&redacted)
}

/// 识别并替换私钥、环境变量里的敏感值与常见 Token。
///
/// 输入按空白切成 token 逐个处理，空白本身保持不变，便于诊断摘要分行阅读。
pub fn redact_secrets(text: &str) -> String {
    if contains_private_key(text) {
        return PRIVATE_KEY_PLACEHOLDER.to_string();
    }

    let mut result = String::with_capacity(text.len());
    let mut token = String::new();
    for character in text.chars() {
        if character.is_whitespace() {
            result.push_str(&redact_token(&token));
            token.clear();
            result.push(character);
        } else {
            token.push(character);
        }
    }
    result.push_str(&redact_token(&token));
    result
}

/// 对单个 token 应用秘密识别规则；未命中时原样返回，不擅自改写。
fn redact_token(token: &str) -> String {
    if token.is_empty() {
        return String::new();
    }
    // URL 先单独处理：它可能同时含 userinfo 与敏感 query，且不应被 `=` 规则拆坏。
    if token.contains("://") {
        return redact_url(token);
    }
    if let Some((key, _value)) = token.split_once('=') {
        if is_sensitive_key(key) {
            return format!("{key}=***");
        }
    }
    if has_credential_prefix(token) {
        return TOKEN_PLACEHOLDER.to_string();
    }
    token.to_string()
}

/// 判断 `KEY=VALUE` 的键名是否表示凭据；大小写与引号都不影响匹配。
fn is_sensitive_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    SENSITIVE_KEY_PARTS.iter().any(|part| upper.contains(part))
}

/// 判断 token 是否以已知的凭据前缀开头。
fn has_credential_prefix(token: &str) -> bool {
    if CREDENTIAL_PREFIXES
        .iter()
        .any(|prefix| token.starts_with(prefix) && token.len() > prefix.len())
    {
        return true;
    }
    SHORT_CREDENTIAL_PREFIXES
        .iter()
        .any(|prefix| token.starts_with(prefix) && token.len() >= prefix.len() + 8)
}

/// 文本里是否包含 PEM 私钥标记；命中后整段替换，避免多行密钥被拆散写入。
fn contains_private_key(text: &str) -> bool {
    text.contains("-----BEGIN") && text.contains("PRIVATE KEY-----")
}

/// 审计/诊断字段的统一入口：脱敏、过滤控制字符并截断。
pub fn redact_field(text: &str) -> String {
    let redacted = redact_url(&redact_secrets(text));
    truncate(&sanitize_text(&redacted), MAX_FIELD_CHARS)
}

/// 按字符数截断过长文本；截断后加 `...` 提示还有内容。
fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut shortened: String = text.chars().take(max_chars).collect();
    shortened.push_str("...");
    shortened
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_hide_embedded_credentials() {
        assert_eq!(
            redact_url("https://user:token@example.com/team/repo.git"),
            "https://***@example.com/team/repo.git"
        );
        assert_eq!(
            redact_url("https://example.com/team/repo.git"),
            "https://example.com/team/repo.git"
        );
        // scp 形式的地址没有 userinfo，保持原样（其中的 git 只是用户名，不是密码）。
        assert_eq!(
            redact_url("git@example.com:team/repo.git"),
            "git@example.com:team/repo.git"
        );
        // 路径里的 `@` 不是凭据。
        assert_eq!(
            redact_url("https://example.com/team/repo@v1.git"),
            "https://example.com/team/repo@v1.git"
        );
    }

    #[test]
    fn urls_with_sensitive_query_values_are_redacted() {
        // query 里的 token 属于凭据：即使没有 userinfo，也要只留键名。
        let redacted = redact_field("https://example.com/repo?access_token=abc123&ref=main");
        assert_eq!(
            redacted,
            "https://example.com/repo?access_token=***&ref=main"
        );
        // fragment 原样保留，不能因为 query 脱敏把整段 URL 截掉。
        assert_eq!(
            redact_url("https://example.com/repo?token=abc#/diff?token=keep"),
            "https://example.com/repo?token=***#/diff?token=keep"
        );
    }

    #[test]
    fn sensitive_assignments_keep_only_the_key() {
        for (input, expected) in [
            ("GITHUB_TOKEN=ghp_secretvalue", "GITHUB_TOKEN=***"),
            ("DB_PASSWORD=hunter2", "DB_PASSWORD=***"),
            ("api_key=abc123", "api_key=***"),
            ("AWS_SECRET_ACCESS_KEY=abc", "AWS_SECRET_ACCESS_KEY=***"),
        ] {
            assert_eq!(redact_secrets(input), expected);
        }
        // 普通字段不能被误伤。
        assert_eq!(redact_secrets("AUTHOR=Ada"), "AUTHOR=Ada");
        assert_eq!(redact_secrets("branch=main"), "branch=main");
    }

    #[test]
    fn known_token_prefixes_are_replaced() {
        for token in [
            "ghp_1234567890abcdef",
            "github_pat_11ABCDEFG",
            "xoxb-123456789",
            "sk-1234567890",
            "AKIAIOSFODNN7EXAMPLE",
        ] {
            assert_eq!(redact_secrets(token), TOKEN_PLACEHOLDER, "{token}");
        }
        // 太短的 `sk-` / `AKIA` 可能只是普通文本，不做处理。
        assert_eq!(redact_secrets("sk-1"), "sk-1");
        assert_eq!(redact_secrets("AKIA1"), "AKIA1");
    }

    #[test]
    fn private_keys_are_replaced_as_a_whole_block() {
        let key = "-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n-----END OPENSSH PRIVATE KEY-----";
        assert_eq!(redact_secrets(key), PRIVATE_KEY_PLACEHOLDER);
        assert_eq!(
            redact_field(key),
            PRIVATE_KEY_PLACEHOLDER,
            "控制字符过滤不应把占位文本拆开"
        );
    }

    #[test]
    fn control_and_bidi_characters_are_replaced_by_spaces() {
        let redacted = sanitize_text("line1\nline2\u{1b}[31mred\u{202e}");
        assert!(!redacted.contains('\n'));
        assert!(!redacted.contains('\u{1b}'));
        assert!(!redacted.contains('\u{202e}'));
        assert!(redacted.contains("[31mred"));
    }

    #[test]
    fn long_fields_are_truncated_by_characters() {
        let redacted = redact_field(&"字".repeat(MAX_FIELD_CHARS + 10));
        assert_eq!(redacted.chars().count(), MAX_FIELD_CHARS + 3);
        assert!(redacted.ends_with("..."));
    }

    #[test]
    fn a_url_with_credentials_is_redacted_even_inside_a_sentence() {
        let redacted = redact_field("remote https://user:pass@example.com/repo.git failed");
        assert!(redacted.contains("https://***@example.com/repo.git"));
        assert!(!redacted.contains("pass"));
    }
}
