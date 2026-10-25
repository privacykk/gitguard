//! 审计事件、统一脱敏、JSONL 存储与诊断摘要。
//!
//! 依赖方向：`gitguard-cli / 桌面端 → gitguard-audit → gitguard-core`。
//!
//! 这一层只做记录与展示准备：
//! - 不判断是否放行，也不读取策略；
//! - 写入失败只向上返回错误，由调用方写 `stderr`，绝不改变退出码；
//! - 审计文件是本地普通文件，可被本机用户修改，不构成防篡改凭证；
//! - 所有写入字段先经过 [`redact`] 里的脱敏规则。
//!
//! 审计只由 CLI 写入（一次推送的请求与结果都在同一个进程里），桌面端只读，
//! 避免同一次推送出现两条互相矛盾的记录。

mod diagnostics;
mod event;
mod redact;
mod store;

pub use diagnostics::{export_report, DiagnosticReport};
pub use event::{
    format_epoch_utc, reason, sanitize_code, AuditChannel, AuditEvent, AuditEventKind, AuditResult,
    AuditSubject,
};
pub use redact::{redact_field, redact_secrets, redact_url, sanitize_text, MAX_FIELD_CHARS};
pub use store::{
    append_event_to, audit_file_path, enforce_retention_at, read_events_from, AuditTail,
    AUDIT_FILE_NAME, MAX_READ_BYTES, MAX_RETENTION_ENTRIES, RETENTION_DAYS,
};
