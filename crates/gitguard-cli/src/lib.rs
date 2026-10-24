//! `gitguard` CLI 的实现（协议层）。
//!
//! 只做四件事：校验参数、完整读取 Hook stdin、逐行解析 ref、把策略结果映射为退出码。
//! 这里不实现策略判断，也不把外部输入拼进 shell 命令。
//!
//! 典型调用方是 Git 的 `pre-push` Hook：
//! `gitguard hook pre-push --policy <allow|deny|confirm|unavailable> [remote-name remote-url]`

use std::io::{Read, Write};

use gitguard_core::{CliExitCode, Decision, GitGuardError, RefUpdate};
use gitguard_policy::{evaluate, PrototypePolicy};

/// 进程入口：把执行结果统一转换成退出码，并保证错误只写 `stderr`。
///
/// - `args` 是 argv（包含程序名），`input` 通常是 Hook stdin，`stderr` 是错误输出；
/// - 只有 `Success` 返回 0；其他判定或错误都会写一行提示并返回非零，
///   因此不存在「出错但 push 继续」的路径；
/// - 泛型参数让单元测试能用内存缓冲区代替真实 IO。
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

/// 一次完整处理：解析参数 → 读完 stdin → 解析 ref → 调用策略 → 映射决策。
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

/// 校验并解析命令行参数。
///
/// - `args` 包含程序名本身（argv[0]），所以先 `skip(1)`；
/// - Git 调用 Hook 时会额外追加 `<remote-name> <remote-url>`，这里只接受
///   「两个都有或都没有」，且只做校验，不会执行 URL 里的内容。
fn parse_args<I, S>(args: I) -> gitguard_core::Result<PrototypePolicy>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args: Vec<String> = args.into_iter().map(Into::into).skip(1).collect();
    // 固定前缀：hook pre-push --policy <值>；缺少任何一段都算用法错误。
    if args.len() < 4 || args[0] != "hook" || args[1] != "pre-push" || args[2] != "--policy" {
        return Err(GitGuardError::InvalidInput(
            "usage: gitguard hook pre-push --policy <allow|deny|confirm|unavailable> [remote-name remote-url]"
                .into(),
        ));
    }

    // 第 5 个参数起是 Git 传入的 remote 名与 URL：要么不传，要么恰好两个。
    let trailing_args = &args[4..];
    if !trailing_args.is_empty() && trailing_args.len() != 2 {
        return Err(GitGuardError::InvalidInput(
            "Git must supply both remote-name and remote-url".into(),
        ));
    }

    args[3].parse()
}

/// 按行解析 Hook stdin。
///
/// stdin 为空说明调用方使用方式不对（或没有待推送的 ref），直接报错；
/// 不能当作「无事发生」而静默放行。
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

/// 解析一行 ref 记录：必须恰好四个字段，且两个 OID 都合法。
///
/// - 先去掉行尾 `\r`（Windows 换行）再按空白切分；
/// - 任意一行非法都会拒绝整个请求，不会只跳过坏行。
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

/// OID 只接受 40 位（SHA-1）或 64 位（SHA-256）十六进制字符串。
/// 这里只校验形状，不校验对象是否真的存在于仓库中。
fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// 错误类型到退出码的唯一映射点：新增错误变体必须同步这里与退出码测试。
fn exit_code_for_error(error: &GitGuardError) -> CliExitCode {
    match error {
        GitGuardError::InvalidInput(_) => CliExitCode::InvalidInput,
        GitGuardError::PolicyUnavailable(_) => CliExitCode::PolicyUnavailable,
        GitGuardError::Io(_) => CliExitCode::IoError,
        GitGuardError::Internal(_) => CliExitCode::InternalError,
    }
}

// 单元测试直接调用 `run` 与解析函数，用 `Cursor` 模拟 stdin，不触碰真实 IO。
#[cfg(test)]
mod tests {
    use std::io::{self, Cursor};

    use super::*;

    /// 全零 OID：Git 用它表示「该位置没有对象」。
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
        // 四种策略结果必须映射为固定退出码（0/1/2/4）。
        assert_eq!(invoke("allow", &ordinary_update()).0, 0);
        assert_eq!(invoke("deny", &ordinary_update()).0, 1);
        assert_eq!(invoke("confirm", &ordinary_update()).0, 2);
        assert_eq!(invoke("unavailable", &ordinary_update()).0, 4);
    }

    #[test]
    fn reads_all_refs_and_rejects_an_invalid_later_line() {
        // 第 1 行合法、第 2 行非法时，不能先处理完第 1 行就返回成功。
        let input = format!("{}\ninvalid\n", ordinary_update().trim_end());
        let (code, stderr) = invoke("allow", &input);
        assert_eq!(code, CliExitCode::InvalidInput.as_i32());
        assert!(stderr.contains("line 2"));
    }

    #[test]
    fn parses_multiple_creation_deletion_and_update_records() {
        // 三种常见形态：普通更新、新建（远端全零）、删除（本地全零）。
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
        // 模拟 stdin 读取失败（如管道断开）与非法 UTF-8，两者都必须 fail-closed。
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
        // 缺少子命令、缺少策略值，或只给 remote 名不给 URL，都算参数错误。
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
        // Git 传入的 URL 只是普通字符串：即使含 `;` 也不能被当成 shell 命令。
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
