use std::io::{Read, Write};

use gitguard_core::{CliExitCode, Decision, GitGuardError, RefUpdate};
use gitguard_policy::{evaluate, PrototypePolicy};

pub fn run<I, S, R, W>(args: I, input: &mut R, stderr: &mut W) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    R: Read,
    W: Write,
{
    match execute(args, input) {
        Ok(CliExitCode::Success) => CliExitCode::Success.as_i32(),
        Ok(exit_code) => {
            let _ = writeln!(stderr, "gitguard: push rejected");
            exit_code.as_i32()
        }
        Err(error) => {
            let exit_code = exit_code_for_error(&error);
            let _ = writeln!(stderr, "gitguard: {error}");
            exit_code.as_i32()
        }
    }
}

fn execute<I, S, R>(args: I, input: &mut R) -> gitguard_core::Result<CliExitCode>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    R: Read,
{
    let policy = parse_args(args)?;
    let mut raw_input = String::new();
    input
        .read_to_string(&mut raw_input)
        .map_err(|error| GitGuardError::Io(format!("could not read hook stdin: {error}")))?;
    let updates = parse_hook_input(&raw_input)?;
    let result = evaluate(policy, &updates)?;

    match result.decision {
        Decision::Allow => Ok(CliExitCode::Success),
        Decision::Deny => Ok(CliExitCode::Denied),
        Decision::RequireConfirmation => Ok(CliExitCode::ConfirmationRequired),
    }
}

fn parse_args<I, S>(args: I) -> gitguard_core::Result<PrototypePolicy>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).skip(1).collect();
    if args.len() < 4 || args[0] != "hook" || args[1] != "pre-push" || args[2] != "--policy" {
        return Err(GitGuardError::InvalidInput(
            "usage: gitguard hook pre-push --policy <allow|deny|confirm|unavailable> [remote-name remote-url]"
                .into(),
        ));
    }

    let trailing_args = &args[4..];
    if !trailing_args.is_empty() && trailing_args.len() != 2 {
        return Err(GitGuardError::InvalidInput(
            "Git must supply both remote-name and remote-url".into(),
        ));
    }

    args[3].parse()
}

fn parse_hook_input(input: &str) -> gitguard_core::Result<Vec<RefUpdate>> {
    if input.is_empty() {
        return Err(GitGuardError::InvalidInput("hook stdin is empty".into()));
    }

    input
        .lines()
        .enumerate()
        .map(|(index, line)| parse_ref_update(line, index + 1))
        .collect()
}

fn parse_ref_update(line: &str, line_number: usize) -> gitguard_core::Result<RefUpdate> {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() != 4 || fields.iter().any(|field| field.is_empty()) {
        return Err(GitGuardError::InvalidInput(format!(
            "hook stdin line {line_number} must contain four fields"
        )));
    }
    if !valid_oid(fields[1]) || !valid_oid(fields[3]) {
        return Err(GitGuardError::InvalidInput(format!(
            "hook stdin line {line_number} contains an invalid object ID"
        )));
    }

    Ok(RefUpdate {
        local_ref: fields[0].into(),
        local_oid: fields[1].into(),
        remote_ref: fields[2].into(),
        remote_oid: fields[3].into(),
    })
}

fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn exit_code_for_error(error: &GitGuardError) -> CliExitCode {
    match error {
        GitGuardError::InvalidInput(_) => CliExitCode::InvalidInput,
        GitGuardError::PolicyUnavailable(_) => CliExitCode::PolicyUnavailable,
        GitGuardError::Io(_) => CliExitCode::IoError,
        GitGuardError::Internal(_) => CliExitCode::InternalError,
    }
}

#[cfg(test)]
mod tests {
    use std::io::{self, Cursor};

    use super::*;

    const ZERO: &str = "0000000000000000000000000000000000000000";
    const OID_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OID_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn args(policy: &str) -> Vec<String> {
        ["gitguard", "hook", "pre-push", "--policy", policy]
            .map(String::from)
            .into()
    }

    fn ordinary_update() -> String {
        format!("refs/heads/main {OID_A} refs/heads/main {OID_B}\n")
    }

    fn invoke(policy: &str, input: &str) -> (i32, String) {
        let mut input = Cursor::new(input.as_bytes());
        let mut stderr = Vec::new();
        let code = run(args(policy), &mut input, &mut stderr);
        (
            code,
            String::from_utf8(stderr).expect("stderr should be UTF-8"),
        )
    }

    #[test]
    fn maps_every_decision_to_a_stable_exit_code() {
        assert_eq!(invoke("allow", &ordinary_update()).0, 0);
        assert_eq!(invoke("deny", &ordinary_update()).0, 1);
        assert_eq!(invoke("confirm", &ordinary_update()).0, 2);
        assert_eq!(invoke("unavailable", &ordinary_update()).0, 4);
    }

    #[test]
    fn reads_all_refs_and_rejects_an_invalid_later_line() {
        let input = format!("{}\ninvalid\n", ordinary_update().trim_end());
        let (code, stderr) = invoke("allow", &input);
        assert_eq!(code, CliExitCode::InvalidInput.as_i32());
        assert!(stderr.contains("line 2"));
    }

    #[test]
    fn parses_multiple_creation_deletion_and_update_records() {
        let input = format!(
            "refs/heads/main {OID_A} refs/heads/main {OID_B}\n\
             refs/heads/new {OID_A} refs/heads/new {ZERO}\r\n\
             (delete) {ZERO} refs/heads/old {OID_B}\n"
        );
        let updates = parse_hook_input(&input).expect("valid hook input");
        assert_eq!(updates.len(), 3);
        assert!(!updates[0].is_creation());
        assert!(updates[1].is_creation());
        assert!(updates[2].is_deletion());
    }

    #[test]
    fn rejects_empty_blank_and_malformed_input() {
        for input in [
            "",
            "\n",
            "refs/heads/main\n",
            &ordinary_update().replace(OID_A, "xyz"),
        ] {
            assert_eq!(invoke("allow", input).0, CliExitCode::InvalidInput.as_i32());
        }
    }

    #[test]
    fn read_errors_and_invalid_utf8_do_not_allow() {
        struct BrokenReader;
        impl Read for BrokenReader {
            fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("broken pipe"))
            }
        }

        let mut stderr = Vec::new();
        assert_eq!(
            run(args("allow"), &mut BrokenReader, &mut stderr),
            CliExitCode::IoError.as_i32()
        );
        assert_eq!(
            run(args("allow"), &mut Cursor::new([0xff]), &mut Vec::new()),
            CliExitCode::IoError.as_i32()
        );
    }

    #[test]
    fn rejects_missing_or_ambiguous_arguments() {
        for arguments in [
            vec!["gitguard"],
            vec!["gitguard", "hook", "pre-push"],
            vec!["gitguard", "hook", "pre-push", "--policy"],
            vec![
                "gitguard", "hook", "pre-push", "--policy", "allow", "origin",
            ],
        ] {
            let code = run(
                arguments,
                &mut Cursor::new(ordinary_update()),
                &mut Vec::new(),
            );
            assert_eq!(code, CliExitCode::InvalidInput.as_i32());
        }
    }

    #[test]
    fn accepts_git_remote_arguments_without_executing_them() {
        let mut arguments = args("allow");
        arguments.extend([
            "origin".into(),
            "file:///tmp/remote.git;not-a-command".into(),
        ]);
        let code = run(
            arguments,
            &mut Cursor::new(ordinary_update()),
            &mut Vec::new(),
        );
        assert_eq!(code, CliExitCode::Success.as_i32());
    }
}
