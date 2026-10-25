//! `gitguard` CLI 的实现（协议层）。
//!
//! 只做协议层工作：校验参数、完整读取 Hook stdin、逐行解析 ref、按配置走确认流程、
//! 把结果映射为稳定退出码。这里不实现策略判断，也不把外部输入拼进 shell 命令。
//!
//! 两种调用方式：
//! - `gitguard hook pre-push <remote-name> <remote-url>`：真实 Hook 模式。
//!   有控制终端（在终端里执行 `git push`）时直接在终端确认；没有控制终端
//!   （SourceTree、VS Code、CI 等）时通过 IPC 请求桌面端确认。
//!   桌面端写出的 Hook 还会额外传 `--repository-id <uuid>`，用于把 Hook 与配置里
//!   的登记记录对应起来；配置记录丢失时据此拒绝静默放行。
//! - `gitguard hook pre-push --policy <allow|deny|confirm|unavailable>`：Week 1 的
//!   原型调试模式，不读配置、不发 IPC，仅用于测试与排查。
//!
//! 无论走哪条确认通道，确认完成后都会重新校验 ref 未被移动，然后返回退出码 0，
//! 让原本的 `git push` 自己把数据推上去——这样终端与图形客户端都能看到真实的
//! 推送进度和凭据交互，也不需要 GitGuard 再执行一次 push。

mod terminal;

use std::io::{Read, Write};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gitguard_config::PushConfirmationState;
use gitguard_core::{
    CliExitCode, ConfirmationDecision, Decision, GitGuardError, PushConfirmationRequest, RefUpdate,
};
use gitguard_policy::{evaluate, PrototypePolicy};
use uuid::Uuid;

/// 确认超时至少留 1 秒：配置里写 0 时仍给桌面端一个响应机会，
/// 超时只会导致 fail-closed，但把 0 直接交给 socket API 会变成参数错误。
const MIN_CONFIRMATION_TIMEOUT_SECS: u64 = 1;

/// 一次执行的最终走向。
///
/// 区分 [`Outcome::Allow`] 与 [`Outcome::AllowAfterConfirmation`] 是为了在
/// 终端上给出准确的提示：前者表示不需要确认（保护关闭或受控推送），
/// 后者表示用户刚刚确认过，接下来由原本的 `git push` 继续完成推送。
enum Outcome {
    /// 明确放行本次 push。
    Allow,
    /// 用户已确认且 ref 已重新校验，退出码 0 让原 Git 进程继续推送。
    AllowAfterConfirmation,
    /// 明确拒绝。
    Reject(CliExitCode),
}

/// 进程入口：把执行结果统一转换成退出码，并保证错误只写 `stderr`。
///
/// - `args` 是 argv（包含程序名），`input` 通常是 Hook stdin，`stderr` 是错误输出；
/// - 只有 `Allow` 返回 0；其他判定或错误都会写一行提示并返回非零，
///   因此不存在「出错但 push 继续」的路径；
/// - 泛型参数让单元测试能用内存缓冲区代替真实 IO。
pub fn run<I, S, R, W>(args: I, input: &mut R, stderr: &mut W) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    R: Read,
    W: Write,
{
    finish(execute(args, input, stderr), stderr)
}

/// 把执行结果写成提示与退出码：这是「结果 → 进程状态」的唯一映射点。
fn finish<W: Write>(outcome: gitguard_core::Result<Outcome>, stderr: &mut W) -> i32 {
    match outcome {
        Ok(Outcome::Allow) => CliExitCode::Success.as_i32(),
        Ok(Outcome::AllowAfterConfirmation) => {
            // 确认之后由原来的 git push 继续发送数据，因此这里必须返回 0。
            let _ = writeln!(
                stderr,
                "gitguard: push confirmed; continuing with the original git push"
            );
            CliExitCode::Success.as_i32()
        }
        Ok(Outcome::Reject(exit_code)) => {
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

/// 一次完整处理：解析参数 → 读完 stdin → 解析 ref → 走原型策略或确认流程。
fn execute<I, S, R, W>(args: I, input: &mut R, stderr: &mut W) -> gitguard_core::Result<Outcome>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    R: Read,
    W: Write,
{
    let args = parse_args(args)?;
    let mut raw_input = String::new();
    input
        .read_to_string(&mut raw_input)
        .map_err(|error| GitGuardError::Io(format!("could not read hook stdin: {error}")))?;
    let updates = parse_hook_input(&raw_input)?;

    match args.policy {
        // 原型调试模式：不读配置、不发 IPC，保持 Week 1 的行为。
        Some(policy) => match evaluate(policy, &updates)?.decision {
            Decision::Allow => Ok(Outcome::Allow),
            Decision::Deny => Ok(Outcome::Reject(CliExitCode::Denied)),
            Decision::RequireConfirmation => Ok(Outcome::Reject(CliExitCode::ConfirmationRequired)),
        },
        None => request_confirmation(&args, &updates, stderr),
    }
}

/// 真实 Hook 模式：按配置与运行环境决定在哪确认。
///
/// 确认通道的选择完全依靠「能不能打开控制终端」：
/// - 终端里执行 `git push`：进程有控制终端，直接在终端展示并确认；
/// - SourceTree / VS Code / CI：没有控制终端，降级到桌面端确认。
///
/// 任何一步失败——读配置、解析仓库、列提交、连不上桌面端、超时、响应非法——
/// 都会向上返回错误并由 [`run`] 转成非零退出码，绝不静默放行。
fn request_confirmation<W: Write>(
    args: &HookArgs,
    updates: &[RefUpdate],
    stderr: &mut W,
) -> gitguard_core::Result<Outcome> {
    let working_dir = std::env::current_dir().map_err(|error| {
        GitGuardError::Io(format!("could not read the current directory: {error}"))
    })?;
    let repo_path = gitguard_git::repo_canonical_path(&working_dir)?;
    let repo_key = path_to_key(&repo_path)?;

    let config = gitguard_config::load_config()?;
    // 判定顺序见 `gitguard_config::push_confirmation_state`：用户显式关掉保护才直接放行；
    // Hook 带着仓库 id 但配置里找不到记录时，按「记录丢失」处理，仍然要求确认。
    match gitguard_config::push_confirmation_state(&config, repo_key, args.repository_id.as_deref())
    {
        PushConfirmationState::Disabled => return Ok(Outcome::Allow),
        PushConfirmationState::Required => {}
        PushConfirmationState::RecordMissing => {
            // 无法判断用户原本的开关，所以不静默放行；提示里不拼仓库路径，避免输出未过滤的输入。
            let _ = writeln!(
                stderr,
                "gitguard: no GitGuard record matches this repository's hook (the config was removed \
                 or the directory moved); confirmation is still required, re-add the repository in \
                 the desktop app to restore its setting"
            );
        }
    }

    let timeout = Duration::from_secs(
        config
            .global
            .confirmation_timeout_seconds
            .max(MIN_CONFIRMATION_TIMEOUT_SECS),
    );

    // 1) 有控制终端：在终端里确认，桌面端不参与本次请求。
    if let Some(outcome) = confirm_in_terminal(&repo_path, args, updates, timeout)? {
        return Ok(outcome);
    }

    // 2) 没有控制终端（图形客户端、CI）：请求桌面端确认。
    confirm_in_desktop(&repo_path, args, updates, timeout, stderr)
}

/// 在控制终端里展示本次推送并等待用户输入。
///
/// 返回 `Ok(None)` 表示当前进程没有控制终端，调用方应降级到桌面端确认。
fn confirm_in_terminal(
    repo_path: &Path,
    args: &HookArgs,
    updates: &[RefUpdate],
    timeout: Duration,
) -> gitguard_core::Result<Option<Outcome>> {
    let decision =
        terminal::confirm_with(|| build_terminal_preview(repo_path, args, updates), timeout)?;
    let Some(decision) = decision else {
        return Ok(None);
    };

    match decision {
        terminal::Decision::Confirmed => {
            // 确认与真正推送之间还有时间差，必须确认 ref 没有被移走。
            gitguard_git::verify_push_preconditions(repo_path, updates)?;
            Ok(Some(Outcome::AllowAfterConfirmation))
        }
        // 取消与超时都是明确的不确认，一律按拒绝处理。
        terminal::Decision::Declined | terminal::Decision::TimedOut => {
            Ok(Some(Outcome::Reject(CliExitCode::Denied)))
        }
    }
}

/// 收集终端确认界面需要的数据（每个 ref 的待推送提交）。
///
/// 列不出提交就无法让用户判断「要推什么」，此时直接返回错误而不是让用户盲签。
fn build_terminal_preview(
    repo_path: &Path,
    args: &HookArgs,
    updates: &[RefUpdate],
) -> gitguard_core::Result<terminal::Preview> {
    let mut refs = Vec::with_capacity(updates.len());
    // 真实模式下 remote 名一定存在（`parse_args` 已校验）；原型模式不走这里。
    let remote_name = args.remote_name.as_deref().unwrap_or_default();
    for update in updates {
        // 删除远端 ref 时本地没有对象，不需要（也无法）列提交。
        let commits = if update.is_deletion() {
            Vec::new()
        } else {
            gitguard_git::list_commits(
                repo_path,
                remote_name,
                &update.remote_oid,
                &update.local_oid,
            )?
        };
        refs.push(terminal::PreviewRef {
            update: update.clone(),
            commits,
            // 只影响风险提示：无法判断时按「可能改写」展示，不改变放行结果。
            rewrites_history: gitguard_git::rewrites_remote_history(repo_path, update),
        });
    }

    Ok(terminal::Preview {
        repository_path: path_to_key(repo_path)?.to_string(),
        remote_name: args.remote_name.clone().unwrap_or_default(),
        remote_url: args.remote_url.clone().unwrap_or_default(),
        refs,
    })
}

/// 没有控制终端时，把请求交给桌面端，等待用户在桌面端确认。
fn confirm_in_desktop<W: Write>(
    repo_path: &Path,
    args: &HookArgs,
    updates: &[RefUpdate],
    timeout: Duration,
    stderr: &mut W,
) -> gitguard_core::Result<Outcome> {
    let request = PushConfirmationRequest {
        request_id: uuid::Uuid::new_v4().to_string(),
        repository_path: path_to_key(repo_path)?.to_string(),
        remote_name: args.remote_name.clone().unwrap_or_default(),
        remote_url: args.remote_url.clone().unwrap_or_default(),
        ref_updates: updates.to_vec(),
        timestamp_epoch_secs: now_epoch_secs(),
    };

    // 图形客户端只能在输出面板里看到这行提示，否则用户不知道卡在哪里。
    let _ = writeln!(
        stderr,
        "gitguard: waiting for confirmation in the GitGuard desktop app (timeout {timeout:?})"
    );

    let socket_path = gitguard_ipc::default_socket_path()?;
    let response = gitguard_ipc::send_confirmation_request(&socket_path, &request, timeout)?;

    // 响应必须对得上本次请求；对不上说明串了会话，按 IPC 失败处理。
    if response.request_id != request.request_id {
        return Err(GitGuardError::IpcError(
            "the desktop app returned a response for a different request".into(),
        ));
    }

    match response.decision {
        ConfirmationDecision::Confirmed => {
            // 真正执行推送的是原始 git push，放行前再校验一次 ref。
            gitguard_git::verify_push_preconditions(repo_path, updates)?;
            Ok(Outcome::AllowAfterConfirmation)
        }
        // 取消、超时、内部错误一律按拒绝处理。
        ConfirmationDecision::Cancelled
        | ConfirmationDecision::Timeout
        | ConfirmationDecision::Error => Ok(Outcome::Reject(CliExitCode::Denied)),
    }
}

/// 把仓库路径转成配置里使用的字符串键。
fn path_to_key(path: &Path) -> gitguard_core::Result<&str> {
    path.to_str().ok_or_else(|| {
        GitGuardError::InvalidInput(format!(
            "repository path {} is not valid UTF-8",
            path.display()
        ))
    })
}

/// 当前 Unix 时间戳（秒）；仅用于展示请求时间，不参与任何判定。
fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// 解析后的 `hook pre-push` 参数。
struct HookArgs {
    /// `Some` 表示 Week 1 原型调试模式（不走配置与 IPC）。
    policy: Option<PrototypePolicy>,
    /// Hook 脚本写入的仓库 id；旧版脚本没有这个参数。
    repository_id: Option<String>,
    /// Git 传入的 remote 名称；原型模式下可以不传。
    remote_name: Option<String>,
    /// Git 传入的 remote URL；只用于展示。
    remote_url: Option<String>,
}

/// 校验并解析命令行参数。
///
/// - `args` 包含程序名本身（argv[0]），所以先 `skip(1)`；
/// - 固定前缀是 `hook pre-push`，其后可选 `--policy <值>` 与 `--repository-id <uuid>`；
/// - Git 调用 Hook 时会额外追加 `<remote-name> <remote-url>`，这里只接受
///   「两个都有或都没有」，且只做校验，不会执行 URL 里的内容；
/// - 真实模式（不带 `--policy`）必须拿到 Git 传入的 remote 信息，
///   否则无法让用户在确认界面上看清目标，直接按用法错误处理。
fn parse_args<I, S>(args: I) -> gitguard_core::Result<HookArgs>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    const USAGE: &str = "usage: gitguard hook pre-push [--policy <allow|deny|confirm|unavailable>] [--repository-id <uuid>] [remote-name remote-url]";

    let args: Vec<String> = args.into_iter().map(Into::into).skip(1).collect();
    if args.len() < 2 || args[0] != "hook" || args[1] != "pre-push" {
        return Err(GitGuardError::InvalidInput(USAGE.into()));
    }

    let mut index = 2;
    let mut policy = None;
    let mut repository_id = None;
    while let Some(flag) = args.get(index).map(String::as_str) {
        match flag {
            "--policy" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(GitGuardError::InvalidInput(USAGE.into()));
                };
                if policy.is_some() {
                    return Err(GitGuardError::InvalidInput(USAGE.into()));
                }
                policy = Some(value.parse()?);
            }
            "--repository-id" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(GitGuardError::InvalidInput(USAGE.into()));
                };
                if repository_id.is_some() {
                    return Err(GitGuardError::InvalidInput(USAGE.into()));
                }
                // 这个值来自仓库里的 Hook 脚本：形状不合法就不接受，避免拿它去匹配配置。
                if Uuid::parse_str(value).is_err() {
                    return Err(GitGuardError::InvalidInput(
                        "--repository-id must be a UUID".into(),
                    ));
                }
                repository_id = Some(value.clone());
            }
            _ => break,
        }
        index += 2;
    }

    // 剩下的参数是 Git 传入的 remote 名与 URL：要么不传，要么恰好两个。
    let (remote_name, remote_url) = match &args[index..] {
        [] => (None, None),
        [name, url] => (Some(name.clone()), Some(url.clone())),
        _ => {
            return Err(GitGuardError::InvalidInput(
                "Git must supply both remote-name and remote-url".into(),
            ));
        }
    };

    if policy.is_none() && remote_name.is_none() {
        return Err(GitGuardError::InvalidInput(
            "the real pre-push mode requires the remote-name and remote-url supplied by Git".into(),
        ));
    }

    Ok(HookArgs {
        policy,
        repository_id,
        remote_name,
        remote_url,
    })
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
        GitGuardError::IpcError(_) => CliExitCode::IpcError,
    }
}

// 单元测试直接调用 `run` 与解析函数，用 `Cursor` 模拟 stdin，不触碰真实 IO。
#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{self, Cursor};
    use std::process::Command;

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
    fn maps_ipc_errors_to_exit_code_seven() {
        // IPC 失败（桌面端未运行、超时等）不能让 push 继续，固定退出码 7。
        let error = GitGuardError::IpcError("desktop app is not running".into());
        assert_eq!(
            exit_code_for_error(&error).as_i32(),
            CliExitCode::IpcError.as_i32()
        );
        assert_eq!(exit_code_for_error(&error).as_i32(), 7);
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

    #[test]
    fn real_mode_requires_the_remote_arguments_git_provides() {
        // 不带 --policy 时是真实 Hook 模式，缺 remote 信息就无法让用户看清推送目标。
        let parsed = parse_args(["gitguard", "hook", "pre-push"]);
        assert!(matches!(parsed, Err(GitGuardError::InvalidInput(_))));
    }

    #[test]
    fn real_mode_parses_remote_arguments() {
        let parsed = parse_args([
            "gitguard",
            "hook",
            "pre-push",
            "origin",
            "git@example.com:team/project.git",
        ])
        .expect("real mode arguments should parse");

        assert!(parsed.policy.is_none(), "真实模式不得隐含任何原型策略");
        assert_eq!(parsed.repository_id, None, "旧版脚本不带仓库 id");
        assert_eq!(parsed.remote_name.as_deref(), Some("origin"));
        assert_eq!(
            parsed.remote_url.as_deref(),
            Some("git@example.com:team/project.git")
        );
    }

    #[test]
    fn parses_the_repository_id_written_into_the_hook() {
        let parsed = parse_args([
            "gitguard",
            "hook",
            "pre-push",
            "--repository-id",
            "00000000-0000-4000-8000-000000000000",
            "origin",
            "git@example.com:team/project.git",
        ])
        .expect("hook arguments with a repository id should parse");

        assert_eq!(
            parsed.repository_id.as_deref(),
            Some("00000000-0000-4000-8000-000000000000")
        );
        assert_eq!(parsed.remote_name.as_deref(), Some("origin"));
    }

    #[test]
    fn rejects_a_repository_id_that_is_not_a_uuid() {
        // 这个值会被拿去匹配配置记录，形状不合法就宁可直接拒绝，也不要猜。
        for arguments in [
            vec!["gitguard", "hook", "pre-push", "--repository-id"],
            vec![
                "gitguard",
                "hook",
                "pre-push",
                "--repository-id",
                "not-a-uuid",
                "origin",
                "file:///tmp/remote.git",
            ],
            vec![
                "gitguard",
                "hook",
                "pre-push",
                "--repository-id",
                "00000000-0000-4000-8000-000000000000",
                "--repository-id",
                "00000000-0000-4000-8000-000000000001",
                "origin",
                "file:///tmp/remote.git",
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
    fn confirmation_lets_the_original_push_continue() {
        // 终端或桌面端确认后，由原来的 git push 继续推送，因此 Hook 必须返回 0。
        let mut stderr = Vec::new();
        let code = finish(Ok(Outcome::AllowAfterConfirmation), &mut stderr);
        assert_eq!(code, CliExitCode::Success.as_i32());
        let message = String::from_utf8(stderr).expect("stderr should be UTF-8");
        assert!(message.contains("confirmed"));
        assert!(!message.contains("rejected"));
    }

    /// 运行 Git 并返回原始输出；三个环境变量隔离用户配置与交互提示。
    fn try_git(cwd: &Path, args: &[&str]) -> std::process::Output {
        Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .expect("execute git")
    }

    fn git(cwd: &Path, args: &[&str]) {
        let output = try_git(cwd, args);
        assert!(
            output.status.success(),
            "git {args:?} failed:\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// 在临时仓库里提交一次并返回完整 OID。
    fn commit(root: &Path, subject: &str) -> String {
        fs::write(root.join("file.txt"), format!("{subject}\n")).expect("write test file");
        git(root, &["add", "file.txt"]);
        git(root, &["commit", "-m", subject]);
        let output = try_git(root, &["rev-parse", "HEAD"]);
        assert!(output.status.success(), "rev-parse HEAD failed");
        String::from_utf8(output.stdout)
            .expect("git stdout should be UTF-8")
            .trim()
            .to_owned()
    }

    #[test]
    fn terminal_preview_marks_non_fast_forward_updates_as_rewrites() {
        // 接线测试：build_terminal_preview 必须把祖先关系结论传进 rewrites_history，
        // 且两处调用共用同一个判断（写反参数会让这里的断言失败）。
        let temp = tempfile::tempdir().expect("create temporary directory");
        let root = temp.path().join("repo");
        fs::create_dir(&root).expect("create repository directory");
        git(&root, &["init", "-b", "main"]);
        git(&root, &["config", "user.name", "GitGuard Test"]);
        git(&root, &["config", "user.email", "gitguard@example.invalid"]);
        let base = commit(&root, "base commit");
        git(&root, &["checkout", "-b", "remote-only", &base]);
        let remote = commit(&root, "remote only commit");
        git(&root, &["checkout", "main"]);
        let local = commit(&root, "local only commit");

        let args = HookArgs {
            policy: None,
            repository_id: None,
            remote_name: Some("origin".into()),
            remote_url: Some("file:///tmp/remote.git".into()),
        };
        let updates = [
            // 远端有本地没有的提交：非快进。
            RefUpdate {
                local_ref: "refs/heads/main".into(),
                local_oid: local.clone(),
                remote_ref: "refs/heads/main".into(),
                remote_oid: remote,
            },
            // 远端提交是本地提交的祖先：快进。
            RefUpdate {
                local_ref: "refs/heads/topic".into(),
                local_oid: local,
                remote_ref: "refs/heads/topic".into(),
                remote_oid: base,
            },
        ];

        let preview = build_terminal_preview(&root, &args, &updates).expect("preview should build");
        assert!(preview.refs[0].rewrites_history);
        assert!(!preview.refs[1].rewrites_history);
    }

    #[test]
    fn rejection_outcomes_keep_their_exit_codes() {
        for (outcome, expected) in [
            (Outcome::Reject(CliExitCode::Denied), 1),
            (Outcome::Reject(CliExitCode::ConfirmationRequired), 2),
        ] {
            assert_eq!(finish(Ok(outcome), &mut Vec::new()), expected);
        }
    }
}
