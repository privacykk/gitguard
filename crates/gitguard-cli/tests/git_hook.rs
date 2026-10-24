//! `pre-push` Hook 的真实 Git 集成测试（仅 Unix）。
//!
//! 每个测试都在 `tempfile` 临时目录里创建「工作仓库 + 本地 bare remote」，
//! 安装只属于该临时仓库的 Hook，然后真实执行 `git push`。
//! 不访问网络、不读取用户全局/系统 Git 配置，也不碰日常仓库；
//! 断言「被拒绝」时同时确认远端 ref 未变化，而不只看退出码。
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

/// 一套隔离的 Git 测试环境：临时目录持有者 + 工作仓库 + 本地 bare remote。
///
/// `_temp` 只用于保持临时目录存活；字段名前缀下划线表示「不直接读取」。
struct GitFixture {
    _temp: TempDir,
    work: PathBuf,
    remote: PathBuf,
}

impl GitFixture {
    /// 初始化工作仓库：创建 bare remote、首次提交，并把 remote 命名为 `origin`。
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("create temporary test directory");
        let work = temp.path().join("work");
        let remote = temp.path().join("remote.git");
        fs::create_dir(&work).expect("create work repository directory");
        git(
            &work,
            &[
                "init",
                "--bare",
                remote.to_str().expect("UTF-8 remote path"),
            ],
        );
        git(&work, &["init", "-b", "main"]);
        git(&work, &["config", "user.name", "GitGuard Test"]);
        git(&work, &["config", "user.email", "gitguard@example.invalid"]);
        git(
            &work,
            &[
                "remote",
                "add",
                "origin",
                remote.to_str().expect("UTF-8 path"),
            ],
        );
        fs::write(work.join("README.md"), "temporary test repository\n").expect("write test file");
        git(&work, &["add", "README.md"]);
        git(&work, &["commit", "-m", "initial commit"]);
        Self {
            _temp: temp,
            work,
            remote,
        }
    }

    /// 在临时仓库里安装 Hook，把 Git 传入的参数原样转交给被测 CLI。
    ///
    /// `env!("CARGO_BIN_EXE_gitguard")` 由 Cargo 在测试时注入，指向刚构建的二进制。
    fn install_policy(&self, policy: &str) {
        // Hook 是 shell 脚本，路径必须先转义再拼进脚本。
        let binary = shell_quote(Path::new(env!("CARGO_BIN_EXE_gitguard")));
        self.install_hook(&format!(
            "#!/bin/sh\nexec {binary} hook pre-push --policy {policy} \"$@\"\n"
        ));
    }

    /// 写入 Hook 脚本并加可执行权限（0755），否则 Git 会忽略它。
    fn install_hook(&self, script: &str) {
        let hook = self.work.join(".git/hooks/pre-push");
        fs::write(&hook, script).expect("install hook in temporary repository");
        let mut permissions = fs::metadata(&hook)
            .expect("read hook metadata")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&hook, permissions).expect("make temporary hook executable");
    }

    /// 在工作仓库产生一次新提交，用来制造「有待推送内容」的场景。
    fn commit_change(&self, contents: &str) {
        fs::write(self.work.join("README.md"), contents).expect("change test file");
        git(&self.work, &["add", "README.md"]);
        git(&self.work, &["commit", "-m", "test change"]);
    }

    fn push(&self, args: &[&str]) -> Output {
        try_git(&self.work, args)
    }

    /// 读取远端 ref 当前指向的 OID，用于断言推送是否真的生效。
    fn remote_oid(&self, reference: &str) -> Vec<u8> {
        git(&self.remote, &["rev-parse", reference]).stdout
    }

    /// 先放行推一次建立基线，再切换到指定策略重推并断言：
    /// push 失败，且远端 ref 与基线完全一致（证明不是「推送成功后再报错」）。
    fn assert_push_blocked_without_ref_change(&self, policy: &str) {
        // 基线：先用 allow 把远端推进到已知状态。
        self.install_policy("allow");
        assert!(self.push(&["push", "origin", "main"]).status.success());
        let before = self.remote_oid("refs/heads/main");
        self.commit_change(&format!("blocked by {policy}\n"));
        self.install_policy(policy);
        let output = self.push(&["push", "origin", "main"]);
        assert!(
            !output.status.success(),
            "{policy} push unexpectedly succeeded"
        );
        assert_eq!(self.remote_oid("refs/heads/main"), before);
    }
}

/// 把路径包成单引号字符串，并转义内部单引号，避免拼出可被注入的脚本。
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

/// 运行 Git 并要求成功；失败时把 stdout/stderr 一并输出，便于定位。
fn git(cwd: &Path, args: &[&str]) -> Output {
    let output = try_git(cwd, args);
    assert!(
        output.status.success(),
        "git {args:?} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// 运行 Git 并返回原始输出。
///
/// 三个环境变量把用户全局/系统配置与交互提示隔离掉，
/// 保证测试结果只依赖临时仓库本身。
fn try_git(cwd: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("execute git")
}

#[test]
fn allow_policy_completes_a_real_push() {
    // allow 时 push 应成功，且远端 ref 与本地指向同一个 OID。
    let fixture = GitFixture::new();
    fixture.install_policy("allow");
    let output = fixture.push(&["push", "origin", "main"]);
    assert!(
        output.status.success(),
        "allow push failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fixture.remote_oid("refs/heads/main"),
        git(&fixture.work, &["rev-parse", "refs/heads/main"]).stdout
    );
}

#[test]
fn deny_policy_blocks_push_and_preserves_remote_ref() {
    // 明确拒绝：push 必须失败，且远端 ref 保持原样。
    GitFixture::new().assert_push_blocked_without_ref_change("deny");
}

#[test]
fn confirmation_without_a_service_blocks_push_and_preserves_remote_ref() {
    // 确认服务尚未实现，需要确认等同于拒绝。
    GitFixture::new().assert_push_blocked_without_ref_change("confirm");
}

#[test]
fn unavailable_policy_blocks_push_and_preserves_remote_ref() {
    // 策略故障时必须 fail-closed，不能静默放行。
    GitFixture::new().assert_push_blocked_without_ref_change("unavailable");
}

#[test]
fn multiple_refs_are_all_pushed() {
    // 一次 push 多个 ref，每个远端 ref 都要与本地一致。
    let fixture = GitFixture::new();
    git(&fixture.work, &["branch", "feature"]);
    fixture.install_policy("allow");
    let output = fixture.push(&["push", "origin", "main", "feature"]);
    assert!(
        output.status.success(),
        "multi-ref push failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for reference in ["refs/heads/main", "refs/heads/feature"] {
        assert_eq!(
            fixture.remote_oid(reference),
            git(&fixture.work, &["rev-parse", reference]).stdout
        );
    }
}

#[test]
fn branch_creation_and_deletion_work_with_real_git() {
    // `:topic` 是 Git 删除远端分支的写法；新建与删除两种形态都要能通过 Hook。
    let fixture = GitFixture::new();
    fixture.install_policy("allow");
    assert!(fixture.push(&["push", "origin", "main"]).status.success());
    git(&fixture.work, &["branch", "topic"]);
    assert!(fixture.push(&["push", "origin", "topic"]).status.success());
    fixture.remote_oid("refs/heads/topic");
    assert!(fixture.push(&["push", "origin", ":topic"]).status.success());
    assert!(!try_git(
        &fixture.remote,
        &["show-ref", "--verify", "refs/heads/topic"]
    )
    .status
    .success());
}

#[test]
fn missing_cli_blocks_push_and_preserves_remote_ref() {
    // Hook 指向不存在的二进制时，Git 因 Hook 非零退出而阻断 push。
    let fixture = GitFixture::new();
    fixture.install_policy("allow");
    assert!(fixture.push(&["push", "origin", "main"]).status.success());
    let before = fixture.remote_oid("refs/heads/main");
    fixture.commit_change("missing CLI must fail closed\n");
    let missing = shell_quote(&fixture.work.join("does-not-exist/gitguard"));
    fixture.install_hook(&format!(
        "#!/bin/sh\nexec {missing} hook pre-push --policy allow \"$@\"\n"
    ));
    assert!(!fixture.push(&["push", "origin", "main"]).status.success());
    assert_eq!(fixture.remote_oid("refs/heads/main"), before);
}

#[test]
fn hook_execution_failure_blocks_push_and_preserves_remote_ref() {
    // Hook 自身退出 42（未明确放行）也必须阻断 push。
    let fixture = GitFixture::new();
    fixture.install_policy("allow");
    assert!(fixture.push(&["push", "origin", "main"]).status.success());
    let before = fixture.remote_oid("refs/heads/main");
    fixture.commit_change("failed hook must fail closed\n");
    fixture.install_hook("#!/bin/sh\nexit 42\n");
    assert!(!fixture.push(&["push", "origin", "main"]).status.success());
    assert_eq!(fixture.remote_oid("refs/heads/main"), before);
}
