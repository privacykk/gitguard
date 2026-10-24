//! 各层共享的基础类型：决策、策略结果、ref 更新、错误与 CLI 退出码。
//!
//! 这里只放纯数据与错误定义：不执行 Git、不读文件、不访问网络，也不依赖
//! Tauri、Vue 或数据库。依赖方向是 `gitguard-cli → gitguard-policy → gitguard-core`。

use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

/// 对一次 push 请求的最终判定。
///
/// 只有 [`Decision::Allow`] 映射为退出码 0；其余取值都让 push 终止（fail-closed）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// 明确放行。
    Allow,
    /// 需要人工确认。确认由 `gitguard-cli` 的终端 / 桌面通道完成；
    /// 没有拿到明确确认时，调用方必须按拒绝处理。
    RequireConfirmation,
    /// 明确拒绝。
    Deny,
}

/// 策略判断结果。
///
/// `reason_code` 是稳定的机器可读标识（如 `explicit_allow`），供测试与后续 UI 使用；
/// 它不随提示文案变化，程序也绝不能靠解析文案来做判断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyResult {
    pub decision: Decision,
    pub reason_code: String,
}

impl PolicyResult {
    /// 构造结果；`reason_code` 接受 `&str` 或 `String`。
    pub fn new(decision: Decision, reason_code: impl Into<String>) -> Self {
        Self {
            decision,
            reason_code: reason_code.into(),
        }
    }
}

/// Git `pre-push` Hook 从 stdin 收到的一行记录。
///
/// 格式为 `local_ref local_oid remote_ref remote_oid`，例如：
/// `refs/heads/main aaaa… refs/heads/main bbbb…`。
/// `oid` 是 Git 对象 ID：SHA-1 为 40 位十六进制，SHA-256 为 64 位。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefUpdate {
    /// 本地 ref 名，如 `refs/heads/main`；删除分支时 Git 传入 `(delete)`。
    pub local_ref: String,
    /// 本地 ref 当前指向的对象 ID；删除时为全零，表示该对象已不存在。
    pub local_oid: String,
    /// 远端 ref 名。
    pub remote_ref: String,
    /// 远端 ref 当前指向的对象 ID；新建分支时为全零。
    pub remote_oid: String,
}

impl RefUpdate {
    /// 远端还没有这个 ref（`remote_oid` 全零），即本次 push 会新建它。
    pub fn is_creation(&self) -> bool {
        is_zero_oid(&self.remote_oid)
    }

    /// 本地 ref 已不存在（`local_oid` 全零），即本次 push 会删除远端 ref。
    pub fn is_deletion(&self) -> bool {
        is_zero_oid(&self.local_oid)
    }
}

/// Git 用「全零 OID」表示该位置没有对象。
/// 空字符串不算全零，避免把非法输入误判为创建或删除。
fn is_zero_oid(oid: &str) -> bool {
    !oid.is_empty() && oid.bytes().all(|byte| byte == b'0')
}

/// IPC 推送确认请求：Hook 发送给桌面端，包含本次 push 的全部上下文。
///
/// `request_id` 唯一标识一次请求，用于关联响应；
/// `ref_updates` 来自 Hook stdin 的 ref 更新列表。
///
/// 注意：这些字段只是普通的字符串数据，展示时直接渲染，绝不会拼进 shell 命令。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushConfirmationRequest {
    /// 唯一请求 ID（UUID v4），用于关联响应。
    pub request_id: String,
    /// 仓库规范路径。
    pub repository_path: String,
    /// Remote 名称，如 `origin`。
    pub remote_name: String,
    /// Remote URL，用于展示（不会拼进 shell 命令）。
    pub remote_url: String,
    /// Hook stdin 解析出的 ref 更新列表。
    pub ref_updates: Vec<RefUpdate>,
    /// 请求时间戳（Unix epoch 秒）。
    pub timestamp_epoch_secs: u64,
}

/// IPC 确认响应：桌面端返回给 Hook。
///
/// Hook 只认 `Confirmed` 对应的一次性批准，其余取值一律按拒绝处理（fail-closed）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushConfirmationResponse {
    /// 与请求对应的 ID。
    pub request_id: String,
    /// 用户的确认决定。
    pub decision: ConfirmationDecision,
    /// 可选的附加消息（如错误说明）。
    pub message: String,
}

/// 用户对推送确认请求的回应。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfirmationDecision {
    /// 用户输入 yes 确认推送。
    Confirmed,
    /// 用户取消推送。
    Cancelled,
    /// 确认超时。
    Timeout,
    /// 处理过程中出错。
    Error,
}

/// `git log` 解析出的单条 Commit 信息，用于确认界面展示。
///
/// 这里只保存展示所需的文本，不包含 diff 内容，也不参与任何判定。
/// 序列化时字段名转成 camelCase（如 `short_sha` → `shortSha`），
/// 因为这份 JSON 唯一的消费方是桌面端界面，前端按 camelCase 校验字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfo {
    /// 完整 SHA。
    pub sha: String,
    /// 缩短的 SHA（通常 7-8 位），用于界面展示。
    pub short_sha: String,
    /// 提交作者名。
    pub author: String,
    /// 提交时间（ISO 8601 格式）。
    pub timestamp: String,
    /// 提交标题（第一行）。
    pub subject: String,
}

/// 可预期错误的统一类型；每个变体只对应一个 CLI 退出码（见 `exit_code_for_error`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitGuardError {
    /// 参数、Hook stdin 或 ref 格式不合法 → 退出码 3。
    InvalidInput(String),
    /// 策略无法给出结论（故障或显式不可用）→ 退出码 4，push 必须被拒绝。
    PolicyUnavailable(String),
    /// stdin/stderr 读写失败 → 退出码 5。
    Io(String),
    /// 未分类的内部错误 → 退出码 6。
    Internal(String),
    /// IPC 通信失败（桌面端未运行、连接超时等）→ 退出码 7。
    IpcError(String),
}

/// 渲染成写进 stderr 的一行文本；程序行为不依赖这段文本，不要解析它。
impl Display for GitGuardError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(formatter, "invalid input: {message}"),
            Self::PolicyUnavailable(message) => write!(formatter, "policy unavailable: {message}"),
            Self::Io(message) => write!(formatter, "I/O error: {message}"),
            Self::Internal(message) => write!(formatter, "internal error: {message}"),
            Self::IpcError(message) => write!(formatter, "IPC error: {message}"),
        }
    }
}

// 实现标准库的 `Error`，让错误可以被 `?` 传播，也能与通用错误处理工具配合。
impl Error for GitGuardError {}

/// 本项目统一使用的 `Result` 别名，错误类型固定为 [`GitGuardError`]。
pub type Result<T> = std::result::Result<T, GitGuardError>;

/// 进程退出码，是与 Git、CI 等外部调用方之间的稳定契约。
///
/// 只有 `Success` 是 0（放行），其余全部非零。Git 看到 Hook 非零退出就会终止 push，
/// 因此任何失败路径都天然 fail-closed。修改数值必须同步 README 与退出码测试。
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliExitCode {
    /// 明确允许，push 继续。
    Success = 0,
    /// 明确拒绝。
    Denied = 1,
    /// 需要确认，但确认服务不可用。
    ConfirmationRequired = 2,
    /// 参数或 Hook stdin 非法。
    InvalidInput = 3,
    /// 策略不可用。
    PolicyUnavailable = 4,
    /// stdin/stderr I/O 错误。
    IoError = 5,
    /// 内部错误。
    InternalError = 6,
    /// IPC 通信失败。
    IpcError = 7,
}

impl CliExitCode {
    /// 转成 `std::process::exit` 需要的 `i32`。
    pub const fn as_i32(self) -> i32 {
        self as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_update_identifies_creation_and_deletion() {
        let zero = "0".repeat(40);
        let oid = "a".repeat(40);
        let update = RefUpdate {
            local_ref: "refs/heads/main".into(),
            local_oid: zero.clone(),
            remote_ref: "refs/heads/old".into(),
            remote_oid: oid.clone(),
        };
        assert!(update.is_deletion());
        assert!(!update.is_creation());

        let update = RefUpdate {
            local_oid: oid,
            remote_oid: zero,
            ..update
        };
        assert!(update.is_creation());
        assert!(!update.is_deletion());
    }

    #[test]
    fn exit_codes_are_stable() {
        assert_eq!(CliExitCode::Success.as_i32(), 0);
        assert_eq!(CliExitCode::Denied.as_i32(), 1);
        assert_eq!(CliExitCode::ConfirmationRequired.as_i32(), 2);
        assert_eq!(CliExitCode::InvalidInput.as_i32(), 3);
        assert_eq!(CliExitCode::PolicyUnavailable.as_i32(), 4);
        assert_eq!(CliExitCode::IoError.as_i32(), 5);
        assert_eq!(CliExitCode::InternalError.as_i32(), 6);
        assert_eq!(CliExitCode::IpcError.as_i32(), 7);
    }

    /// 序列化再反序列化后必须与原值完全相等，用来锁住 IPC 字段名与结构。
    fn assert_json_round_trip<T>(value: T)
    where
        T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_string(&value).expect("value should serialize to JSON");
        let restored: T =
            serde_json::from_str(&json).expect("serialized JSON should deserialize back");
        assert_eq!(value, restored);
    }

    fn sample_ref_update() -> RefUpdate {
        RefUpdate {
            local_ref: "refs/heads/main".into(),
            local_oid: "a".repeat(40),
            remote_ref: "refs/heads/main".into(),
            remote_oid: "b".repeat(40),
        }
    }

    #[test]
    fn ref_update_round_trips_through_json() {
        assert_json_round_trip(sample_ref_update());
    }

    #[test]
    fn push_confirmation_request_round_trips_through_json() {
        let request = PushConfirmationRequest {
            request_id: "6f1c5d2e-4b8a-4c62-9dc0-1d2f3a4b5c6d".into(),
            repository_path: "/Users/dev/project".into(),
            remote_name: "origin".into(),
            remote_url: "git@example.com:team/project.git".into(),
            ref_updates: vec![sample_ref_update()],
            timestamp_epoch_secs: 1_760_000_000,
        };
        assert_json_round_trip(request);
    }

    #[test]
    fn push_confirmation_response_round_trips_through_json() {
        // 四个决定都必须能无损往返，Hook 才能正确区分确认与其余拒绝情形。
        for decision in [
            ConfirmationDecision::Confirmed,
            ConfirmationDecision::Cancelled,
            ConfirmationDecision::Timeout,
            ConfirmationDecision::Error,
        ] {
            let response = PushConfirmationResponse {
                request_id: "6f1c5d2e-4b8a-4c62-9dc0-1d2f3a4b5c6d".into(),
                decision,
                message: String::new(),
            };
            assert_json_round_trip(response);
        }
    }

    fn sample_commit_info() -> CommitInfo {
        CommitInfo {
            sha: "a".repeat(40),
            short_sha: "a".repeat(8),
            author: "Example Author".into(),
            timestamp: "2026-10-24T09:30:00+08:00".into(),
            subject: "feat(cli): add tests".into(),
        }
    }

    #[test]
    fn commit_info_round_trips_through_json() {
        assert_json_round_trip(sample_commit_info());
    }

    #[test]
    fn commit_info_json_uses_the_camel_case_names_the_desktop_app_expects() {
        // 桌面端按 `shortSha` 校验字段形状，字段名一旦回退成 `short_sha`，
        // 确认界面就会把这个请求当作「没有待确认请求」，而 Hook 仍在等待超时。
        let json = serde_json::to_value(sample_commit_info()).expect("commit should serialize");
        assert!(
            json.get("shortSha").is_some(),
            "JSON 必须使用 shortSha 字段名"
        );
        assert!(
            json.get("short_sha").is_none(),
            "不应再出现 snake_case 的 short_sha 字段"
        );
        assert_eq!(
            json.get("author").and_then(|value| value.as_str()),
            Some("Example Author")
        );
    }
}
