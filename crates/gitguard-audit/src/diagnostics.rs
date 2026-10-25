//! 诊断摘要：把配置、IPC 与最近审计事件整理成可直接粘贴的文本。
//!
//! 摘要在 Rust 侧生成并完成脱敏，前端只负责复制或导出，不再自行拼接字段。
//! 摘要不包含仓库内容、提交信息或环境变量，只包含运行状态与最近事件。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use gitguard_core::Result;

use crate::event::{format_epoch_utc, sanitize_code, AuditEvent};
use crate::redact::redact_field;
use crate::store::write_new_private;

/// 一份诊断摘要的全部字段。
///
/// 由桌面端填充；所有自由文本在 [`Self::render`] 里统一脱敏。
#[derive(Debug, Clone)]
pub struct DiagnosticReport {
    pub generated_at_epoch_secs: u64,
    pub app_version: String,
    pub platform: String,
    pub config_path: String,
    pub config_error: Option<String>,
    pub ipc_error: Option<String>,
    pub repositories: usize,
    pub protected_repositories: usize,
    pub audit_path: String,
    pub audit_entries: usize,
    pub audit_skipped_lines: usize,
    pub audit_error: Option<String>,
    pub last_error_code: Option<String>,
    /// 最近事件（旧 → 新）；渲染时按新 → 旧列出。
    pub recent_events: Vec<AuditEvent>,
}

impl DiagnosticReport {
    /// 渲染成稳定格式的纯文本；所有文本字段都已脱敏。
    pub fn render(&self) -> String {
        let mut output = String::from("gitguard diagnostics\n");
        output.push_str(&format!(
            "generated_at: {}\n",
            format_epoch_utc(self.generated_at_epoch_secs)
        ));
        output.push_str(&format!(
            "generated_at_epoch_secs: {}\n",
            self.generated_at_epoch_secs
        ));
        output.push_str(&format!(
            "app_version: {}\n",
            redact_field(&self.app_version)
        ));
        output.push_str(&format!("platform: {}\n", redact_field(&self.platform)));
        output.push_str(&format!(
            "config_path: {}\n",
            redact_field(&self.config_path)
        ));
        output.push_str(&format!(
            "config_error: {}\n",
            optional_code(self.config_error.as_deref())
        ));
        output.push_str(&format!(
            "ipc_error: {}\n",
            optional_code(self.ipc_error.as_deref())
        ));
        output.push_str(&format!("repositories: {}\n", self.repositories));
        output.push_str(&format!(
            "protected_repositories: {}\n",
            self.protected_repositories
        ));
        output.push_str(&format!("audit_path: {}\n", redact_field(&self.audit_path)));
        output.push_str(&format!("audit_entries: {}\n", self.audit_entries));
        output.push_str(&format!(
            "audit_skipped_lines: {}\n",
            self.audit_skipped_lines
        ));
        output.push_str(&format!(
            "audit_error: {}\n",
            optional_code(self.audit_error.as_deref())
        ));
        output.push_str(&format!(
            "last_error_code: {}\n",
            optional_code(self.last_error_code.as_deref())
        ));

        output.push_str("recent_events:\n");
        if self.recent_events.is_empty() {
            output.push_str("  none\n");
            return output;
        }
        for event in self.recent_events.iter().rev() {
            output.push_str(&format!(
                "  {} event={} result={} reason={} channel={} repository={} repository_id={} remote={} remote_url={} request_id={} refs={}\n",
                format_epoch_utc(event.timestamp_epoch_secs),
                event.event.as_str(),
                event.result.as_str(),
                sanitize_code(&event.reason_code),
                event.channel.as_str(),
                redact_field(&event.repository_path),
                optional_code(event.repository_id.as_deref()),
                redact_field(&event.remote_name),
                redact_field(&event.remote_url),
                optional_code(event.request_id.as_deref()),
                event.ref_count,
            ));
        }
        output
    }
}

/// 把可选的错误码/请求 id 收窄为稳定字符串；缺省时写 `none`。
fn optional_code(value: Option<&str>) -> String {
    value
        .map(sanitize_code)
        .unwrap_or_else(|| "none".to_string())
}

/// 把摘要写成 `<配置目录>/diagnostics-<毫秒时间戳>.txt`，返回实际路径。
///
/// 目标文件已存在时拒绝覆盖，交给调用方提示用户再试一次。
pub fn export_report(report: &DiagnosticReport, config_dir: &Path) -> Result<PathBuf> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or(0);
    let path = config_dir.join(format!("diagnostics-{millis}.txt"));
    write_new_private(&path, report.render().as_bytes())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{reason, AuditChannel, AuditEventKind, AuditResult, AuditSubject};

    fn report() -> DiagnosticReport {
        let subject = AuditSubject::new(
            "/tmp/example",
            Some("00000000-0000-4000-8000-000000000000"),
            "origin",
            "https://user:super-secret@example.com/team/repo.git",
            1,
        );
        let event = subject.event(
            1_700_000_000,
            AuditEventKind::RequestCreated,
            AuditResult::Pending,
            reason::CONFIRMATION_REQUIRED,
            Some("6f1c5d2e-4b8a-4c62-9dc0-1d2f3a4b5c6d"),
            AuditChannel::Desktop,
        );

        DiagnosticReport {
            generated_at_epoch_secs: 1_700_000_100,
            app_version: "0.1.0".into(),
            platform: "macos".into(),
            config_path: "/tmp/config.json".into(),
            config_error: None,
            ipc_error: Some("ipc_unavailable".into()),
            repositories: 1,
            protected_repositories: 1,
            audit_path: "/tmp/audit.jsonl".into(),
            audit_entries: 1,
            audit_skipped_lines: 0,
            audit_error: None,
            last_error_code: Some("request_invalid".into()),
            recent_events: vec![event],
        }
    }

    #[test]
    fn rendered_report_contains_state_without_credentials() {
        let text = report().render();

        assert!(text.starts_with("gitguard diagnostics\n"));
        assert!(text.contains("ipc_error: ipc_unavailable"));
        assert!(text.contains("last_error_code: request_invalid"));
        assert!(text.contains("recent_events:"));
        assert!(text.contains("https://***@example.com/team/repo.git"));
        assert!(!text.contains("super-secret"));
        assert!(!text.contains("password"));
    }

    #[test]
    fn empty_recent_events_are_stated_explicitly() {
        let mut report = report();
        report.recent_events.clear();
        let text = report.render();
        assert!(text.contains("recent_events:\n  none\n"));
    }

    #[test]
    fn unexpected_codes_are_narrowed_instead_of_leaking_text() {
        let mut report = report();
        report.last_error_code = Some("bad code with spaces".into());
        report.ipc_error = Some("TOKEN=ghp_1234567890".into());
        let text = report.render();

        assert!(text.contains("last_error_code: unknown"));
        assert!(text.contains("ipc_error: unknown"));
        assert!(!text.contains("ghp_"));
    }

    #[test]
    fn export_writes_a_private_file_in_the_config_directory() {
        let dir = tempfile::tempdir().expect("create temporary directory");
        let path = export_report(&report(), dir.path()).expect("export report");

        assert_eq!(path.parent(), Some(dir.path()));
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        assert!(name.starts_with("diagnostics-"), "{name}");
        assert!(name.ends_with(".txt"), "{name}");

        let text = std::fs::read_to_string(&path).expect("read exported report");
        assert!(text.contains("gitguard diagnostics"));
        assert!(!text.contains("super-secret"));
    }
}
