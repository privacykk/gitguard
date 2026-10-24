//! Week 1 的原型策略层：把命令行的 `--policy` 选项翻译成明确的决策。
//!
//! 这是纯逻辑层：不弹窗、不访问 UI/数据库、不执行命令、不读时间或随机数，
//! 也不得在「无法判断」时返回 `Allow`。

use std::str::FromStr;

use gitguard_core::{Decision, GitGuardError, PolicyResult, RefUpdate, Result};

/// 原型阶段由 `--policy` 显式指定的策略。
///
/// Week 1 没有正式的策略配置与确认服务，因此不提供任何默认值：
/// 不传 `--policy` 就是参数错误，而不是默认放行。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrototypePolicy {
    /// 直接放行；仅用于演示与测试。
    Allow,
    /// 直接拒绝。
    Deny,
    /// 需要确认；确认服务未实现，最终仍会被拒绝。
    RequireConfirmation,
    /// 模拟策略故障，用于验证 fail-closed 路径。
    Unavailable,
}

/// 从命令行字符串解析策略；未知取值返回参数错误，绝不回退到 `allow`。
impl FromStr for PrototypePolicy {
    type Err = GitGuardError;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "allow" => Ok(Self::Allow),
            "deny" => Ok(Self::Deny),
            "confirm" => Ok(Self::RequireConfirmation),
            "unavailable" => Ok(Self::Unavailable),
            _ => Err(GitGuardError::InvalidInput(format!(
                "unknown policy '{value}'; expected allow, deny, confirm, or unavailable"
            ))),
        }
    }
}

/// 根据策略与本次 push 的 ref 列表给出判断。
///
/// - 没有任何 ref 视为调用方出错，返回 `InvalidInput`；
/// - `Unavailable` 走错误通道而不是 `Deny`，让 CLI 用不同的退出码区分
///   策略故障与明确拒绝（fail-closed 但不混淆原因）。
///
/// 返回值里的 `reason_code` 是稳定标识，供测试与后续 UI 使用。
pub fn evaluate(policy: PrototypePolicy, updates: &[RefUpdate]) -> Result<PolicyResult> {
    if updates.is_empty() {
        return Err(GitGuardError::InvalidInput(
            "at least one ref update is required".into(),
        ));
    }

    match policy {
        PrototypePolicy::Allow => Ok(PolicyResult::new(Decision::Allow, "explicit_allow")),
        PrototypePolicy::Deny => Ok(PolicyResult::new(Decision::Deny, "explicit_deny")),
        PrototypePolicy::RequireConfirmation => Ok(PolicyResult::new(
            Decision::RequireConfirmation,
            "confirmation_required",
        )),
        PrototypePolicy::Unavailable => Err(GitGuardError::PolicyUnavailable(
            "the prototype policy was explicitly marked unavailable".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn updates() -> Vec<RefUpdate> {
        vec![RefUpdate {
            local_ref: "refs/heads/main".into(),
            local_oid: "a".repeat(40),
            remote_ref: "refs/heads/main".into(),
            remote_oid: "b".repeat(40),
        }]
    }

    #[test]
    fn returns_explicit_decisions_and_reason_codes() {
        let cases = [
            (PrototypePolicy::Allow, Decision::Allow, "explicit_allow"),
            (PrototypePolicy::Deny, Decision::Deny, "explicit_deny"),
            (
                PrototypePolicy::RequireConfirmation,
                Decision::RequireConfirmation,
                "confirmation_required",
            ),
        ];

        for (policy, decision, reason) in cases {
            let result = evaluate(policy, &updates()).expect("policy should evaluate");
            assert_eq!(result.decision, decision);
            assert_eq!(result.reason_code, reason);
        }
    }

    #[test]
    fn unavailable_and_empty_input_fail_closed() {
        assert!(matches!(
            evaluate(PrototypePolicy::Unavailable, &updates()),
            Err(GitGuardError::PolicyUnavailable(_))
        ));
        assert!(matches!(
            evaluate(PrototypePolicy::Allow, &[]),
            Err(GitGuardError::InvalidInput(_))
        ));
    }

    #[test]
    fn rejects_unknown_policy() {
        assert!("implicit-allow".parse::<PrototypePolicy>().is_err());
    }
}
