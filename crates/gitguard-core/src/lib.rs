use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    RequireConfirmation,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyResult {
    pub decision: Decision,
    pub reason_code: String,
}

impl PolicyResult {
    pub fn new(decision: Decision, reason_code: impl Into<String>) -> Self {
        Self {
            decision,
            reason_code: reason_code.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefUpdate {
    pub local_ref: String,
    pub local_oid: String,
    pub remote_ref: String,
    pub remote_oid: String,
}

impl RefUpdate {
    pub fn is_creation(&self) -> bool {
        is_zero_oid(&self.remote_oid)
    }

    pub fn is_deletion(&self) -> bool {
        is_zero_oid(&self.local_oid)
    }
}

fn is_zero_oid(oid: &str) -> bool {
    !oid.is_empty() && oid.bytes().all(|byte| byte == b'0')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitGuardError {
    InvalidInput(String),
    PolicyUnavailable(String),
    Io(String),
    Internal(String),
}

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

impl Error for GitGuardError {}

pub type Result<T> = std::result::Result<T, GitGuardError>;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliExitCode {
    Success = 0,
    Denied = 1,
    ConfirmationRequired = 2,
    InvalidInput = 3,
    PolicyUnavailable = 4,
    IoError = 5,
    InternalError = 6,
}

impl CliExitCode {
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
