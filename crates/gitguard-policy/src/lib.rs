use std::str::FromStr;

use gitguard_core::{Decision, GitGuardError, PolicyResult, RefUpdate, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrototypePolicy {
    Allow,
    Deny,
    RequireConfirmation,
    Unavailable,
}

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
