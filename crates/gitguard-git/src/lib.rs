//! Git 操作层：把 GitGuard 需要的 Git 调用集中在一个 crate 里。
//!
//! 依赖方向是 `gitguard-git → gitguard-core`：本 crate 只负责读取仓库信息、
//! 列出待推送的提交、校验 ref 是否变化，不做任何策略判断，不执行 push，
//! 也不依赖 Tauri、Vue 或数据库。
//!
//! 所有命令都通过 [`std::process::Command`] 的参数数组执行，不经过 shell；
//! 每个命令都设置 `GIT_TERMINAL_PROMPT=0`，避免在无人值守时卡在凭据输入上。

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use gitguard_core::{CommitInfo, GitGuardError, RefUpdate, Result};

/// `git log` 字段分隔符：`\x1f`（unit separator，单元分隔符）几乎不会出现在提交信息里，
/// 用它分隔字段比用空格更可靠（作者名或标题本身可能包含空格）。
const FIELD_SEPARATOR: char = '\x1f';

/// `git log` 每行的字段顺序：完整 SHA、短 SHA、作者、ISO 8601 时间、标题。
const LOG_FORMAT: &str = "--format=%H%x1f%h%x1f%an%x1f%aI%x1f%s";

/// 返回 `working_dir` 所在 Git 仓库的根目录（绝对路径）。
///
/// 通过 `git rev-parse --show-toplevel` 解析；`working_dir` 可以是仓库里的任意子目录。
/// 不是 Git 仓库、输出为空或输出不是 UTF-8 时返回 [`GitGuardError::Internal`]，
/// 调用方必须按失败处理，不能猜测一个默认目录继续执行。
pub fn repo_canonical_path(working_dir: &Path) -> Result<PathBuf> {
    let output = run_git(working_dir, &["rev-parse", "--show-toplevel"]).map_err(|error| {
        GitGuardError::Internal(format!("could not resolve repository root: {error}"))
    })?;
    if !output.status.success() {
        return Err(GitGuardError::Internal(format!(
            "could not resolve repository root: {}",
            stderr_text(&output)
        )));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| GitGuardError::Internal("repository root path is not valid UTF-8".into()))?;
    let path = stdout.trim();
    if path.is_empty() {
        return Err(GitGuardError::Internal(
            "git rev-parse --show-toplevel returned an empty path".into(),
        ));
    }

    Ok(PathBuf::from(path))
}

/// 返回仓库实际使用的 Hook 目录（绝对路径）。
///
/// 通过 `git rev-parse --git-path hooks` 解析，因此会尊重用户设置的 `core.hooksPath`；
/// 拿到相对路径时以 `repo_path` 为基准拼成绝对路径，避免调用方在错误的目录下找 Hook。
/// 命令失败、输出为空或不是 UTF-8 时返回 [`GitGuardError::Internal`]。
pub fn hooks_path(repo_path: &Path) -> Result<PathBuf> {
    let output = run_git(repo_path, &["rev-parse", "--git-path", "hooks"]).map_err(|error| {
        GitGuardError::Internal(format!("could not resolve hooks path: {error}"))
    })?;
    if !output.status.success() {
        return Err(GitGuardError::Internal(format!(
            "could not resolve hooks path: {}",
            stderr_text(&output)
        )));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| GitGuardError::Internal("hooks path is not valid UTF-8".into()))?;
    let path = stdout.trim();
    if path.is_empty() {
        return Err(GitGuardError::Internal(
            "git rev-parse --git-path hooks returned an empty path".into(),
        ));
    }

    let resolved = PathBuf::from(path);
    // `--git-path` 在部分场景下返回相对路径，必须相对仓库根目录解析。
    if resolved.is_absolute() {
        Ok(resolved)
    } else {
        Ok(repo_path.join(resolved))
    }
}

/// 列出从 `old_oid` 到 `new_oid` 之间、需要展示给用户的提交。
///
/// - `new_oid` 必须是合法的 40 位（SHA-1）或 64 位（SHA-256）十六进制对象 ID；
///   先校验再交给 Git，避免把未校验的外部输入当作命令行参数。
/// - `old_oid` 全零表示「远端还没有这个 ref」（新建分支），此时列出本地存在、
///   目标 remote 还没有的提交；否则列出 `old_oid..new_oid` 区间内的提交。
/// - `remote_name` 是 Git 传给 Hook 的 remote 名，用于排除「目标远端已有」的提交。
///   这里不能写死 `origin`：推送到 `upstream` 等远端时，用 `origin` 的跟踪分支排除
///   会把目标远端已有的提交也列进预览。名称为空时退化为排除所有 remote。
/// - Git 执行失败返回 [`GitGuardError::Internal`]；命令成功但没有输出时返回空列表。
pub fn list_commits(
    repo_path: &Path,
    remote_name: &str,
    old_oid: &str,
    new_oid: &str,
) -> Result<Vec<CommitInfo>> {
    if !valid_oid(new_oid) {
        return Err(GitGuardError::InvalidInput(format!(
            "new object ID is invalid: {new_oid}"
        )));
    }

    let mut args: Vec<&str> = vec!["log", LOG_FORMAT];
    let range;
    let remotes;
    if is_zero_oid(old_oid) {
        // 新建分支：远端还没有这个 ref，用 `--not --remotes=<name>` 排除目标远端已有的提交。
        // 整个实参由固定前缀拼接而成，remote 名不会被 Git 当成独立选项。
        remotes = if remote_name.is_empty() {
            "--remotes".to_string()
        } else {
            format!("--remotes={remote_name}")
        };
        args.extend([new_oid, "--not", remotes.as_str()]);
    } else {
        if !valid_oid(old_oid) {
            return Err(GitGuardError::InvalidInput(format!(
                "old object ID is invalid: {old_oid}"
            )));
        }
        range = format!("{old_oid}..{new_oid}");
        args.push(range.as_str());
    }

    let output = run_git(repo_path, &args)
        .map_err(|error| GitGuardError::Internal(format!("could not list commits: {error}")))?;
    if !output.status.success() {
        return Err(GitGuardError::Internal(format!(
            "could not list commits: {}",
            stderr_text(&output)
        )));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| GitGuardError::Internal("git log output is not valid UTF-8".into()))?;
    Ok(parse_commit_lines(&stdout))
}

/// 确认「用户看到并同意的 ref」在执行 push 前没有变化。
///
/// 对每个非删除的 ref 更新执行 `git rev-parse --verify <local_ref>`，与请求里的
/// `local_oid` 比较；只要有一个对不上（ref 被移动、重命名或删除），就返回
/// [`GitGuardError::InvalidInput`]，调用方必须放弃这次 push 并重新确认。
/// 删除远端 ref 时本地没有对应对象，直接跳过。
pub fn verify_push_preconditions(repo_path: &Path, ref_updates: &[RefUpdate]) -> Result<()> {
    for update in ref_updates {
        if update.is_deletion() {
            continue;
        }
        validate_ref_name(&update.local_ref)?;
        if !valid_oid(&update.local_oid) {
            return Err(GitGuardError::InvalidInput(format!(
                "ref {} has an invalid local object ID",
                update.local_ref
            )));
        }

        let output = run_git(
            repo_path,
            &["rev-parse", "--verify", update.local_ref.as_str()],
        )?;
        // 读取失败通常意味着 ref 已经被删除，和「值变了」一样都不能继续推送。
        if !output.status.success() {
            return Err(ref_changed(&update.local_ref));
        }

        let current = String::from_utf8(output.stdout)
            .map_err(|_| GitGuardError::Internal("git rev-parse output is not UTF-8".into()))?;
        if !current.trim().eq_ignore_ascii_case(&update.local_oid) {
            return Err(ref_changed(&update.local_ref));
        }
    }

    Ok(())
}

/// 「ref 在确认请求之后发生了变化」这一错误的唯一构造点，保证提示文案一致。
fn ref_changed(local_ref: &str) -> GitGuardError {
    GitGuardError::InvalidInput(format!(
        "ref {local_ref} has changed since confirmation request"
    ))
}

/// ref 名来自 Hook stdin，属于外部输入：拒绝空值、以 `-` 开头的值、内嵌 NUL，
/// 以及会被 refspec 语法误读的空白与 `:`。
/// 参数数组只能防止 shell 注入，以 `-` 开头的字符串仍可能被 Git 当成命令行选项。
fn validate_ref_name(local_ref: &str) -> Result<()> {
    if local_ref.is_empty()
        || local_ref.starts_with('-')
        || local_ref.contains('\0')
        || local_ref.contains(':')
        || local_ref.chars().any(char::is_whitespace)
    {
        return Err(GitGuardError::InvalidInput(
            "local ref name is invalid".into(),
        ));
    }
    Ok(())
}

/// OID 只接受 40 位（SHA-1）或 64 位（SHA-256）十六进制字符串。
/// 这里只校验形状，不校验对象是否真的存在于仓库中。
fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Git 用「全零 OID」表示该位置没有对象；空字符串不算全零，避免把非法输入误判为创建或删除。
fn is_zero_oid(oid: &str) -> bool {
    !oid.is_empty() && oid.bytes().all(|byte| byte == b'0')
}

/// 解析 `git log` 的全部输出，每行一条提交。
///
/// 字段数量不是 5 的行直接跳过：Git 的告警等杂散文本不应该让整个列表失败，
/// 也不应该被误当成一条提交展示给用户。
fn parse_commit_lines(stdout: &str) -> Vec<CommitInfo> {
    stdout.lines().filter_map(parse_commit_line).collect()
}

/// 解析单行提交记录；只有恰好 5 个非空字段才算合法。
fn parse_commit_line(line: &str) -> Option<CommitInfo> {
    let mut fields = line.split(FIELD_SEPARATOR);
    let sha = fields.next()?;
    let short_sha = fields.next()?;
    let author = fields.next()?;
    let timestamp = fields.next()?;
    let subject = fields.next()?;
    // 还有剩余字段说明分隔符数量不对，按坏行跳过。
    if fields.next().is_some() || sha.is_empty() {
        return None;
    }

    Some(CommitInfo {
        sha: sha.into(),
        short_sha: short_sha.into(),
        author: author.into(),
        timestamp: timestamp.into(),
        subject: subject.into(),
    })
}

/// 在 `repo_path` 中执行 git 并返回原始输出。
///
/// 参数以数组形式传入，Git 不经过 shell，因此 ref、路径里的特殊字符不会被解释。
/// `GIT_TERMINAL_PROMPT=0` 让 Git 在需要凭据时立即失败，而不是等待交互输入。
fn run_git(repo_path: &Path, args: &[&str]) -> Result<Output> {
    Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|error| GitGuardError::Io(format!("failed to execute git: {error}")))
}

/// 取出 Git 的 stderr 作为可读错误信息；stderr 为空时退化为退出状态。
fn stderr_text(output: &Output) -> String {
    let text = String::from_utf8_lossy(&output.stderr);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        format!("git exited with {}", output.status)
    } else {
        trimmed.to_owned()
    }
}

// 单元测试既覆盖纯函数，也会在 `tempfile` 临时仓库里执行真实 Git，
// 不访问网络、不读取用户全局/系统 Git 配置。
#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;

    use tempfile::TempDir;

    use super::*;

    const ZERO_OID: &str = "0000000000000000000000000000000000000000";
    const OID_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OID_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    /// 一套隔离的临时 Git 仓库夹具；`_temp` 只用于保持临时目录存活。
    struct TestRepo {
        _temp: TempDir,
        root: PathBuf,
    }

    impl TestRepo {
        fn new() -> Self {
            let temp = tempfile::tempdir().expect("create temporary test directory");
            let root = temp.path().join("repo");
            fs::create_dir(&root).expect("create repository directory");
            git(&root, &["init", "-b", "main"]);
            git(&root, &["config", "user.name", "GitGuard Test"]);
            git(&root, &["config", "user.email", "gitguard@example.invalid"]);
            Self { _temp: temp, root }
        }

        /// 产生一次提交并返回它的完整 SHA，方便测试里构造 OID。
        fn commit(&self, subject: &str) -> String {
            fs::write(self.root.join("file.txt"), format!("{subject}\n")).expect("write test file");
            git(&self.root, &["add", "file.txt"]);
            git(&self.root, &["commit", "-m", subject]);
            git_stdout(&self.root, &["rev-parse", "HEAD"])
                .trim()
                .to_owned()
        }

        /// 创建一个空的 bare remote，命名为 `name`，便于测试多个远端。
        fn add_bare_remote(&self, name: &str) -> PathBuf {
            let remote = self._temp.path().join(format!("{name}.git"));
            let remote_path = remote.to_str().expect("UTF-8 remote path");
            git(self._temp.path(), &["init", "--bare", remote_path]);
            git(&self.root, &["remote", "add", name, remote_path]);
            remote
        }

        /// 创建一个空的 bare remote，并命名为 `origin`。
        fn add_bare_origin(&self) -> PathBuf {
            self.add_bare_remote("origin")
        }
    }

    /// 运行 Git 并返回原始输出；三个环境变量把用户配置与交互提示隔离掉。
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

    /// 运行 Git 并要求成功，失败时输出 stderr 便于定位。
    fn git(cwd: &Path, args: &[&str]) {
        let output = try_git(cwd, args);
        assert!(
            output.status.success(),
            "git {args:?} failed:\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// 运行 Git 并要求成功，返回 UTF-8 stdout。
    fn git_stdout(cwd: &Path, args: &[&str]) -> String {
        let output = try_git(cwd, args);
        assert!(
            output.status.success(),
            "git {args:?} failed:\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("git stdout should be UTF-8")
    }

    /// 构造一条指向 `local_ref` 的 ref 更新，远端 OID 用全零表示新建。
    fn ref_update(local_ref: &str, local_oid: &str) -> RefUpdate {
        RefUpdate {
            local_ref: local_ref.into(),
            local_oid: local_oid.into(),
            remote_ref: local_ref.into(),
            remote_oid: ZERO_OID.into(),
        }
    }

    #[test]
    fn valid_oid_accepts_sha1_and_sha256_lengths() {
        assert!(valid_oid(OID_A));
        assert!(valid_oid(&"0123456789abcdef".repeat(4)));
        assert!(!valid_oid(""));
        assert!(!valid_oid(&"a".repeat(39)));
        assert!(!valid_oid(&"a".repeat(41)));
        assert!(!valid_oid(&"g".repeat(40)));
    }

    #[test]
    fn zero_oid_requires_non_empty_all_zero_string() {
        assert!(is_zero_oid(ZERO_OID));
        assert!(is_zero_oid(&"0".repeat(64)));
        assert!(!is_zero_oid(""));
        assert!(!is_zero_oid(&format!("{}1", "0".repeat(39))));
        assert!(!is_zero_oid(OID_A));
    }

    #[test]
    fn parses_commit_lines_and_skips_malformed_rows() {
        // 两行合法记录之间混入普通文本、空行和字段数量不对的行，都应被跳过。
        let stdout = format!(
            "{OID_A}\x1faaaaaaa\x1fAlice\x1f2026-10-24T09:30:00+08:00\x1ffeat: first\n\
             not a commit line\n\
             {OID_B}\x1fbbbbbbb\x1fBob\x1f2026-10-24T10:00:00+08:00\x1ffix: second\n\
             extra\x1ffields\x1fare\x1fignored\x1fhere\x1fand-more\n\
             \n"
        );

        let commits = parse_commit_lines(&stdout);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].sha, OID_A);
        assert_eq!(commits[0].short_sha, "aaaaaaa");
        assert_eq!(commits[0].author, "Alice");
        assert_eq!(commits[0].subject, "feat: first");
        assert_eq!(commits[1].sha, OID_B);
        assert_eq!(commits[1].timestamp, "2026-10-24T10:00:00+08:00");
        assert_eq!(commits[1].subject, "fix: second");
    }

    #[test]
    fn resolves_repository_root_from_a_subdirectory() {
        let repo = TestRepo::new();
        repo.commit("initial commit");
        let nested = repo.root.join("nested/deeper");
        fs::create_dir_all(&nested).expect("create nested directory");

        let resolved = repo_canonical_path(&nested).expect("resolve repository root");
        // macOS 的临时目录位于符号链接下，两边都做一次 canonicalize 再比较。
        assert_eq!(
            resolved.canonicalize().expect("canonicalize resolved root"),
            repo.root
                .canonicalize()
                .expect("canonicalize expected root")
        );
    }

    #[test]
    fn resolves_the_hooks_directory_inside_the_repository() {
        let repo = TestRepo::new();
        repo.commit("initial commit");

        let hooks = hooks_path(&repo.root).expect("resolve hooks path");
        assert!(hooks.is_absolute(), "Hook 目录必须是绝对路径");
        assert_eq!(
            hooks.file_name().and_then(|name| name.to_str()),
            Some("hooks")
        );
        // macOS 的临时目录位于符号链接下，两边都做一次 canonicalize 再比较。
        assert!(hooks
            .canonicalize()
            .expect("canonicalize hooks path")
            .starts_with(repo.root.canonicalize().expect("canonicalize root")));
    }

    #[test]
    fn non_repository_directory_reports_internal_error() {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let error = repo_canonical_path(temp.path()).expect_err("not a git repository");
        assert!(matches!(error, GitGuardError::Internal(_)));
    }

    #[test]
    fn lists_commits_between_two_oids() {
        let repo = TestRepo::new();
        let first = repo.commit("initial commit");
        let second = repo.commit("second commit");

        let commits = list_commits(&repo.root, "origin", &first, &second).expect("list commits");
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].sha, second);
        assert_eq!(commits[0].subject, "second commit");
        assert!(!commits[0].short_sha.is_empty());
        assert!(commits[0].timestamp.contains('T'));
    }

    #[test]
    fn lists_all_commits_for_a_new_branch() {
        let repo = TestRepo::new();
        repo.commit("initial commit");
        let head = repo.commit("second commit");
        repo.add_bare_origin();

        let commits = list_commits(&repo.root, "origin", ZERO_OID, &head).expect("list commits");
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].subject, "second commit");
        assert_eq!(commits[1].subject, "initial commit");
    }

    #[test]
    fn new_branch_listing_excludes_commits_the_target_remote_already_has() {
        // 回归：曾经写死 `--remotes=origin`，推送到 upstream 时会把目标远端已有的提交
        // 也列进预览，用户看到的「待推送内容」与实际推送范围不一致。
        let repo = TestRepo::new();
        repo.commit("initial commit");
        let head = repo.commit("second commit");
        repo.add_bare_remote("origin");
        repo.add_bare_remote("upstream");
        // upstream 已经拥有全部历史，origin 还是空的。
        git(&repo.root, &["push", "upstream", "main"]);

        let on_origin = list_commits(&repo.root, "origin", ZERO_OID, &head).expect("list commits");
        assert_eq!(on_origin.len(), 2, "origin 没有任何提交，应全部列为待推送");

        let on_upstream =
            list_commits(&repo.root, "upstream", ZERO_OID, &head).expect("list commits");
        assert!(
            on_upstream.is_empty(),
            "upstream 上已有的提交不得再出现在待推送列表: {on_upstream:?}"
        );
    }

    #[test]
    fn rejects_malformed_oids_before_running_git() {
        let repo = TestRepo::new();
        repo.commit("initial commit");

        assert!(matches!(
            list_commits(&repo.root, "origin", ZERO_OID, "not-an-oid"),
            Err(GitGuardError::InvalidInput(_))
        ));
        assert!(matches!(
            list_commits(&repo.root, "origin", "not-an-oid", OID_B),
            Err(GitGuardError::InvalidInput(_))
        ));
    }

    #[test]
    fn verify_preconditions_accepts_unchanged_refs_and_skips_deletions() {
        let repo = TestRepo::new();
        let head = repo.commit("initial commit");
        let updates = [
            ref_update("refs/heads/main", &head),
            ref_update("(delete)", ZERO_OID),
        ];

        verify_push_preconditions(&repo.root, &updates).expect("preconditions should hold");
    }

    #[test]
    fn verify_preconditions_rejects_a_ref_that_moved() {
        let repo = TestRepo::new();
        let stale = repo.commit("initial commit");
        repo.commit("second commit");

        let error = verify_push_preconditions(&repo.root, &[ref_update("refs/heads/main", &stale)])
            .expect_err("stale ref must be rejected");
        assert!(matches!(error, GitGuardError::InvalidInput(_)));
        assert!(error
            .to_string()
            .contains("has changed since confirmation request"));
    }

    #[test]
    fn verify_preconditions_rejects_missing_refs_and_option_like_names() {
        let repo = TestRepo::new();
        repo.commit("initial commit");

        let missing = ref_update("refs/heads/does-not-exist", OID_A);
        assert!(matches!(
            verify_push_preconditions(&repo.root, &[missing]),
            Err(GitGuardError::InvalidInput(_))
        ));

        // 以 `-` 开头的 ref 名可能被 Git 当成选项，必须在执行前拒绝。
        let injected = ref_update("--help", OID_A);
        assert!(matches!(
            verify_push_preconditions(&repo.root, &[injected]),
            Err(GitGuardError::InvalidInput(_))
        ));
    }
}
