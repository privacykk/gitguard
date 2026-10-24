//! 各层共享的基础类型：决策、策略结果、ref 更新、错误与 CLI 退出码。
//!
//! 这里只放纯数据与错误定义：不执行 Git、不读文件、不访问网络，也不依赖
//! Tauri、Vue 或数据库。依赖方向是 `gitguard-cli → gitguard-policy → gitguard-core`。

use std::error::Error;
use std::fmt::{Display, Formatter};

/// 对一次 push 请求的最终判定。
///
/// 只有 [`Decision::Allow`] 映射为退出码 0；其余取值都让 push 终止（fail-closed）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// 明确放行。
    Allow,
    /// 需要人工确认。确认服务尚未实现，调用方必须按拒绝处理。
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
}

/// 渲染成写进 stderr 的一行文本；程序行为不依赖这段文本，不要解析它。
impl Display for GitGuardError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(formatter, "invalid input: {message}"),
            Self::PolicyUnavailable(message) => write!(formatter, "policy unavailable: {message}"),
            Self::Io(message) => write!(formatter, "I/O error: {message}"),
            Self::Internal(message) => write!(formatter, "internal error: {message}"),
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
    }
}
