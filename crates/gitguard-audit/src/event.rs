//! 审计事件模型：一次推送确认从创建到结果的全部记录。
//!
//! 字段全部是普通字符串与枚举，写入前统一经过 [`crate::redact`] 脱敏。
//! 事件只描述「发生了什么」，不参与任何放行判断；审计缺失或损坏时，
//! 调用方仍按原有的 fail-closed 规则处理。

use serde::{Deserialize, Serialize};

use crate::redact::redact_field;

/// 一次推送确认里发生的事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventKind {
    /// 已选定确认通道并创建请求。
    RequestCreated,
    /// 用户明确确认。
    Confirmed,
    /// 用户取消或关闭了确认界面。
    Cancelled,
    /// 等待确认超时。
    Timeout,
    /// 确认流程本身失败（通道不可用、IPC 失败、ref 变化等）。
    Failure,
}

impl AuditEventKind {
    /// 稳定的字符串形式；同时用于 JSON 序列化与诊断摘要。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RequestCreated => "request_created",
            Self::Confirmed => "confirmed",
            Self::Cancelled => "cancelled",
            Self::Timeout => "timeout",
            Self::Failure => "failure",
        }
    }
}

/// 本次事件对应的推送结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditResult {
    /// 请求仍在等待确认，或通道失败后尚未得到决定。
    Pending,
    /// 本次推送被放行（由原始 `git push` 继续）。
    Allowed,
    /// 本次推送被拒绝。
    Rejected,
}

impl AuditResult {
    /// 稳定的字符串形式。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Allowed => "allowed",
            Self::Rejected => "rejected",
        }
    }
}

/// 用户在哪条通道上确认。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditChannel {
    /// 当前终端的 `/dev/tty`。
    Terminal,
    /// 桌面端（Unix Domain Socket）。
    Desktop,
}

impl AuditChannel {
    /// 稳定的字符串形式。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Desktop => "desktop",
        }
    }
}

/// 稳定原因码。文案可以变化，程序与界面判断只依赖这些字符串。
pub mod reason {
    pub const CONFIRMATION_REQUIRED: &str = "confirmation_required";
    pub const USER_CONFIRMED: &str = "user_confirmed";
    pub const USER_CANCELLED: &str = "user_cancelled";
    pub const CONFIRMATION_TIMEOUT: &str = "confirmation_timeout";
    pub const TERMINAL_UNAVAILABLE: &str = "terminal_unavailable";
    pub const IPC_UNAVAILABLE: &str = "ipc_unavailable";
    pub const CONFIRMATION_FAILED: &str = "confirmation_failed";
    pub const REQUEST_MISMATCH: &str = "request_mismatch";
    pub const REFS_CHANGED: &str = "refs_changed";
    pub const COMMIT_LIST_UNAVAILABLE: &str = "commit_list_unavailable";
    pub const INVALID_INPUT: &str = "invalid_input";
    pub const IO_ERROR: &str = "io_error";
    pub const POLICY_UNAVAILABLE: &str = "policy_unavailable";
    pub const INTERNAL_ERROR: &str = "internal_error";
}

/// 一条审计事件，对应 `audit.jsonl` 里的一行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// 事件时间（Unix epoch 秒）。
    pub timestamp_epoch_secs: u64,
    pub event: AuditEventKind,
    pub result: AuditResult,
    /// 稳定原因码，见 [`reason`]。
    pub reason_code: String,
    pub channel: AuditChannel,
    /// 仓库规范路径（脱敏后）。
    pub repository_path: String,
    /// Hook 脚本携带的仓库 id；终端通道或旧脚本可能没有。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    pub remote_name: String,
    /// Remote URL（已隐去凭据）。
    pub remote_url: String,
    /// 桌面端请求 id；终端通道没有。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Hook stdin 里的 ref 更新数量。
    pub ref_count: usize,
}

/// 一次推送的固定上下文；构造时完成脱敏，之后每条事件复用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditSubject {
    repository_path: String,
    repository_id: Option<String>,
    remote_name: String,
    remote_url: String,
    ref_count: usize,
}

impl AuditSubject {
    /// 记录仓库、remote 与 ref 数量；所有字段在构造时脱敏。
    pub fn new(
        repository_path: &str,
        repository_id: Option<&str>,
        remote_name: &str,
        remote_url: &str,
        ref_count: usize,
    ) -> Self {
        Self {
            repository_path: redact_field(repository_path),
            repository_id: repository_id.map(redact_field),
            remote_name: redact_field(remote_name),
            remote_url: redact_field(remote_url),
            ref_count,
        }
    }

    /// 生成一条带完整上下文的事件。
    pub fn event(
        &self,
        timestamp_epoch_secs: u64,
        event: AuditEventKind,
        result: AuditResult,
        reason_code: &str,
        request_id: Option<&str>,
        channel: AuditChannel,
    ) -> AuditEvent {
        AuditEvent {
            timestamp_epoch_secs,
            event,
            result,
            reason_code: sanitize_code(reason_code),
            channel,
            repository_path: self.repository_path.clone(),
            repository_id: self.repository_id.clone(),
            remote_name: self.remote_name.clone(),
            remote_url: self.remote_url.clone(),
            request_id: request_id.map(redact_field),
            ref_count: self.ref_count,
        }
    }
}

/// 把原因码/错误码收窄成 `[a-z0-9_]{1,64}`；不合规时返回 `unknown`。
///
/// 这些码来自本仓库常量或桌面端错误码，仍然按外部输入处理：
/// 不合法就替换，而不是让任意文本进入日志与界面。
pub fn sanitize_code(code: &str) -> String {
    let trimmed = code.trim();
    let valid = !trimmed.is_empty()
        && trimmed.len() <= 64
        && trimmed
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if valid {
        trimmed.to_string()
    } else {
        "unknown".to_string()
    }
}

/// 把 Unix 时间戳格式化成 UTC 文本（`2026-10-25T09:30:00Z`）。
///
/// 只用于展示，不依赖时区库；时间戳异常大时按 0 处理，避免溢出。
pub fn format_epoch_utc(epoch_secs: u64) -> String {
    const SECONDS_PER_DAY: u64 = 24 * 60 * 60;
    let days = (epoch_secs / SECONDS_PER_DAY) as i64;
    let seconds_of_day = epoch_secs % SECONDS_PER_DAY;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds_of_day / 3600,
        (seconds_of_day % 3600) / 60,
        seconds_of_day % 60
    )
}

/// Howard Hinnant 的 `civil_from_days`：Unix 天数 → 公历年月日。
fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let shifted = days_since_epoch + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_epoch_as_utc() {
        assert_eq!(format_epoch_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_epoch_utc(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(format_epoch_utc(1_760_000_000), "2025-10-09T08:53:20Z");
    }

    #[test]
    fn event_context_is_redacted_at_construction() {
        let subject = AuditSubject::new(
            "/tmp/repo",
            Some("00000000-0000-4000-8000-000000000000"),
            "origin",
            "https://user:secret@example.com/repo.git?access_token=abc",
            2,
        );
        let event = subject.event(
            1_700_000_000,
            AuditEventKind::RequestCreated,
            AuditResult::Pending,
            reason::CONFIRMATION_REQUIRED,
            Some("request-1"),
            AuditChannel::Desktop,
        );

        assert_eq!(event.repository_path, "/tmp/repo");
        assert_eq!(event.remote_name, "origin");
        assert_eq!(
            event.remote_url,
            "https://***@example.com/repo.git?access_token=***"
        );
        assert!(!event.remote_url.contains("secret"));
        assert!(!event.remote_url.contains("abc"));
        assert_eq!(event.ref_count, 2);
    }

    #[test]
    fn events_round_trip_through_json_with_stable_names() {
        let subject = AuditSubject::new("/tmp/repo", None, "origin", "git@example.com:a/b.git", 1);
        let event = subject.event(
            1_700_000_000,
            AuditEventKind::Timeout,
            AuditResult::Rejected,
            reason::CONFIRMATION_TIMEOUT,
            None,
            AuditChannel::Terminal,
        );
        let json = serde_json::to_string(&event).expect("event should serialize");
        assert!(json.contains("\"event\":\"timeout\""), "{json}");
        assert!(json.contains("\"result\":\"rejected\""), "{json}");
        assert!(json.contains("\"channel\":\"terminal\""), "{json}");
        assert!(!json.contains("request_id"), "缺省字段不应出现：{json}");

        let restored: AuditEvent = serde_json::from_str(&json).expect("event should deserialize");
        assert_eq!(restored, event);
    }

    #[test]
    fn reason_codes_are_narrowed_to_a_stable_alphabet() {
        assert_eq!(sanitize_code("user_confirmed"), "user_confirmed");
        assert_eq!(sanitize_code("  ipc_unavailable  "), "ipc_unavailable");
        assert_eq!(sanitize_code("User Confirmed"), "unknown");
        assert_eq!(sanitize_code("token=abc"), "unknown");
        assert_eq!(sanitize_code(""), "unknown");
        assert_eq!(sanitize_code(&"a".repeat(65)), "unknown");
    }
}
