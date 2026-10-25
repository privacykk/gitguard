//! `pre-push` Hook 的真实 Git 集成测试（仅 Unix）。
//!
//! 每个测试都在 `tempfile` 临时目录里创建「工作仓库 + 本地 bare remote」，
//! 安装只属于该临时仓库的 Hook，然后真实执行 `git push`。
//! 不访问网络、不读取用户全局/系统 Git 配置，也不碰日常仓库；
//! 断言「被拒绝」时同时确认远端 ref 未变化，而不只看退出码。
#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Arc;

use gitguard_core::{ConfirmationDecision, PushConfirmationRequest, PushConfirmationResponse};
use gitguard_ipc::{IpcHandler, IpcServer};
use tempfile::TempDir;

/// 测试配置里固定的仓库 id。
///
/// 桌面端写出的 Hook 会把自己的仓库 id 传给 CLI，CLI 靠它把 Hook 与配置记录对上。
const REGISTERED_REPOSITORY_ID: &str = "00000000-0000-4000-8000-000000000000";

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

    /// 返回一个隔离的 HOME，让被测进程把配置写到临时目录而不是用户真实目录。
    fn isolated_home(&self) -> PathBuf {
        let home = self._temp.path().join("home");
        fs::create_dir_all(&home).expect("create isolated home directory");
        home
    }

    /// 在隔离的 HOME 下写入 GitGuard 配置，决定这个仓库是否启用推送确认。
    fn write_config(&self, home: &Path, enabled: bool) {
        self.write_config_with_id(home, REGISTERED_REPOSITORY_ID, enabled);
    }

    /// 同 [`Self::write_config`]，但允许指定登记记录里的仓库 id。
    fn write_config_with_id(&self, home: &Path, repository_id: &str, enabled: bool) {
        let canonical = self
            .work
            .canonicalize()
            .expect("canonicalize work repository");
        // 与 `gitguard-config` 相同的目录规则：macOS 用 Application Support，
        // 其余 Unix 用配置目录（受 XDG_CONFIG_HOME 控制）。
        let config_dir = if cfg!(target_os = "macos") {
            home.join("Library/Application Support/dev.gitguard.desktop")
        } else {
            home.join(".config/gitguard")
        };
        fs::create_dir_all(&config_dir).expect("create config directory");

        let config = serde_json::json!({
            "schema_version": 1,
            "global": {
                "require_confirmation_for_unregistered_repositories": true,
                "confirmation_timeout_seconds": 180
            },
            "repositories": [{
                "repository_id": repository_id,
                "canonical_path": canonical.to_str().expect("UTF-8 canonical path"),
                "push_confirmation_enabled": enabled,
                "hook_status": "Unknown"
            }]
        });
        fs::write(
            config_dir.join("config.json"),
            serde_json::to_vec_pretty(&config).expect("serialize test config"),
        )
        .expect("write test config");
    }

    /// 返回隔离 HOME 下与被测进程一致的 IPC socket 路径。
    ///
    /// `dirs::config_dir()` 在 macOS 用 `~/Library/Application Support`，
    /// 其余 Unix 用 `$XDG_CONFIG_HOME`（`run_cli` 已把它指向临时目录）。
    fn socket_path(&self, home: &Path) -> PathBuf {
        let config_dir = if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else {
            home.join(".config")
        };
        config_dir
            .join("dev.gitguard.desktop")
            .join("gitguard.sock")
    }

    /// 安装不带 `--policy` 的真实模式 Hook（Git 会额外追加 remote 参数）。
    ///
    /// 与桌面端写出的脚本一致：带上仓库 id，CLI 因此能判断这条 Hook 属于哪次登记。
    fn install_real_hook(&self) {
        self.install_real_hook_with_id(REGISTERED_REPOSITORY_ID);
    }

    /// 同 [`Self::install_real_hook`]，但指定脚本携带的仓库 id。
    fn install_real_hook_with_id(&self, repository_id: &str) {
        let binary = shell_quote(Path::new(env!("CARGO_BIN_EXE_gitguard")));
        self.install_hook(&format!(
            "#!/bin/sh\nexec {binary} hook pre-push --repository-id {repository_id} \"$@\"\n"
        ));
    }

    /// 安装升级前写出的旧版 Hook：不带仓库 id，只能按路径匹配配置。
    fn install_legacy_real_hook(&self) {
        let binary = shell_quote(Path::new(env!("CARGO_BIN_EXE_gitguard")));
        self.install_hook(&format!("#!/bin/sh\nexec {binary} hook pre-push \"$@\"\n"));
    }

    /// 在隔离的 HOME 下执行 Git；Hook 及其子进程会继承这些变量。
    fn push_with_home(&self, home: &Path, args: &[&str]) -> Output {
        self.push_with_env(home, args, &[])
    }

    /// 同 [`Self::push_with_home`]，额外向 Git 及其 Hook 注入环境变量。
    fn push_with_env(&self, home: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
        let mut command = Command::new("git");
        command
            .args(args)
            .current_dir(&self.work)
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0");
        for (key, value) in envs {
            command.env(key, value);
        }
        command.output().expect("execute git")
    }

    /// 直接运行 gitguard 二进制，并向它提供 Hook stdin。
    fn run_cli(&self, home: &Path, args: &[&str], stdin: &str, envs: &[(&str, &str)]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_gitguard"));
        command
            .args(args)
            .current_dir(&self.work)
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", home.join(".config"));
        for (key, value) in envs {
            command.env(key, value);
        }

        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn gitguard binary");

        child
            .stdin
            .take()
            .expect("child stdin should be piped")
            .write_all(stdin.as_bytes())
            .expect("write hook stdin");
        child.wait_with_output().expect("wait for gitguard")
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
    // 原型模式只输出策略判断，没有任何人可以回答确认，因此必然 fail-closed。
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

#[test]
fn real_mode_exits_with_ipc_error_when_no_desktop_app_is_running() {
    // 配置要求确认，但隔离的 HOME 里没有 socket：必须以 IPC 错误（7）退出。
    let fixture = GitFixture::new();
    let home = fixture.isolated_home();
    // 保护是逐仓库显式开启的；这里显式写入一条启用保护的记录。
    fixture.write_config(&home, true);
    let local = git(&fixture.work, &["rev-parse", "HEAD"]).stdout;
    let local = String::from_utf8(local)
        .expect("UTF-8 oid")
        .trim()
        .to_owned();
    let stdin = format!(
        "refs/heads/main {local} refs/heads/main {}\n",
        "0".repeat(40)
    );

    let output = fixture.run_cli(
        &home,
        &["hook", "pre-push", "origin", "file:///tmp/remote.git"],
        &stdin,
        &[],
    );

    assert_eq!(
        output.status.code(),
        Some(7),
        "桌面端未运行时应固定退出码 7，实际: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("IPC error"));
}

#[test]
fn real_mode_blocks_push_when_the_desktop_app_is_not_running() {
    // 端到端：Hook 拦截 → IPC 失败 → Git 终止 push，且远端 ref 从未被创建。
    let fixture = GitFixture::new();
    let home = fixture.isolated_home();
    fixture.write_config(&home, true);
    fixture.install_real_hook();

    let output = fixture.push_with_home(&home, &["push", "origin", "main"]);
    assert!(
        !output.status.success(),
        "没有确认服务时 push 不得成功: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !try_git(
            &fixture.remote,
            &["show-ref", "--verify", "refs/heads/main"]
        )
        .status
        .success(),
        "被拒绝的 push 不得在远端留下 ref"
    );
}

#[test]
fn real_mode_blocks_push_when_the_hook_record_is_missing() {
    // 回归：配置文件被删除或回滚后，仓库里的 Hook 仍在。脚本带着仓库 id，
    // 但配置里找不到对应记录 —— 无法判断用户原本的开关，所以必须拒绝静默放行：
    // 没有确认服务时 push 直接失败，远端不会留下 ref。
    let fixture = GitFixture::new();
    let home = fixture.isolated_home();
    fixture.install_real_hook();

    let output = fixture.push_with_home(&home, &["push", "origin", "main"]);
    assert!(
        !output.status.success(),
        "记录丢失时不得静默放行: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !try_git(
            &fixture.remote,
            &["show-ref", "--verify", "refs/heads/main"]
        )
        .status
        .success(),
        "被拒绝的 push 不得在远端留下 ref"
    );
}

#[test]
fn real_mode_blocks_push_when_the_hook_id_does_not_match_the_record() {
    // 记录还在、但 id 对不上（配置回滚、仓库被复制）：同样按记录丢失处理，
    // 不能因为「路径还在配置里」就沿用那条记录的开关。
    let fixture = GitFixture::new();
    let home = fixture.isolated_home();
    fixture.write_config(&home, false);
    fixture.install_real_hook_with_id("11111111-1111-4111-8111-111111111111");

    let output = fixture.push_with_home(&home, &["push", "origin", "main"]);
    assert!(
        !output.status.success(),
        "id 不匹配时不得静默放行: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !try_git(
            &fixture.remote,
            &["show-ref", "--verify", "refs/heads/main"]
        )
        .status
        .success(),
        "被拒绝的 push 不得在远端留下 ref"
    );
}

#[test]
fn real_mode_allows_a_legacy_hook_without_a_record_to_fall_back_to_the_global_default() {
    // 已知残余路径：升级前写出的脚本不带仓库 id，又恰好丢了配置记录时，
    // 只能回落到全局开关（默认关闭），这是 docs/SECURITY.md 记录的削弱点。
    let fixture = GitFixture::new();
    let home = fixture.isolated_home();
    fixture.install_legacy_real_hook();

    let output = fixture.push_with_home(&home, &["push", "origin", "main"]);
    assert!(
        output.status.success(),
        "旧版脚本只能回落到全局默认: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fixture.remote_oid("refs/heads/main"),
        git(&fixture.work, &["rev-parse", "refs/heads/main"]).stdout
    );
}

#[test]
fn real_mode_allows_push_when_protection_is_disabled_for_the_repository() {
    // 仓库级关闭保护后，Hook 直接放行，不依赖桌面端是否运行。
    let fixture = GitFixture::new();
    let home = fixture.isolated_home();
    fixture.write_config(&home, false);
    fixture.install_real_hook();

    let output = fixture.push_with_home(&home, &["push", "origin", "main"]);
    assert!(
        output.status.success(),
        "关闭保护后 push 应成功: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fixture.remote_oid("refs/heads/main"),
        git(&fixture.work, &["rev-parse", "refs/heads/main"]).stdout
    );
}

#[test]
fn legacy_controlled_push_marker_no_longer_bypasses_confirmation() {
    // 回归：该标记曾是 CLI 里的静默放行开关，与 Hook 脚本里的同类判断构成两处绕过。
    // 现在放行只来自确认流程本身：设置它不得改变结果，也不能在远端留下 ref。
    let fixture = GitFixture::new();
    let home = fixture.isolated_home();
    fixture.write_config(&home, true);
    fixture.install_real_hook();

    let output = fixture.push_with_env(
        &home,
        &["push", "origin", "main"],
        &[("GITGUARD_CONTROLLED_PUSH", "1")],
    );

    assert!(
        !output.status.success(),
        "设置旧版标记不得让 push 通过: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !try_git(
            &fixture.remote,
            &["show-ref", "--verify", "refs/heads/main"]
        )
        .status
        .success(),
        "被拒绝的 push 不得在远端留下 ref"
    );
}

/// 测试用 IPC 处理器：总是返回 `Confirmed`，用来验证确认路径本身。
struct ConfirmingHandler;

impl IpcHandler for ConfirmingHandler {
    fn handle_push_request(&self, request: PushConfirmationRequest) -> PushConfirmationResponse {
        PushConfirmationResponse {
            request_id: request.request_id,
            decision: ConfirmationDecision::Confirmed,
            message: "approved in test".to_string(),
        }
    }
}

/// 创建短路径的隔离 HOME。
///
/// Unix socket 的 `sun_path` 有长度上限（macOS 必须 < 104 字节），系统默认
/// 临时目录太深，因此只在 /tmp 下为 HOME 建临时目录；仓库仍在 fixture 里。
fn short_isolated_home() -> TempDir {
    tempfile::Builder::new()
        .prefix("gitguard-home-")
        .tempdir_in("/tmp")
        .expect("create a short isolated home")
}

#[test]
fn confirmed_non_fast_forward_update_exits_zero_without_pushing() {
    // 回归：风险标记只用于展示，不得改变结论。非快进更新在用户确认后仍
    // 必须返回 0，由原来的 git push 完成推送；CLI 自己不得执行 push。
    let fixture = GitFixture::new();
    let home_dir = short_isolated_home();
    let home = home_dir.path();
    fixture.write_config(home, true);

    // 先建立远端基线，再让远端多出一个本地没有的提交。
    assert!(fixture.push(&["push", "origin", "main"]).status.success());
    let base = String::from_utf8(git(&fixture.work, &["rev-parse", "HEAD"]).stdout)
        .expect("UTF-8 oid")
        .trim()
        .to_owned();
    fixture.commit_change("remote only change\n");
    assert!(fixture.push(&["push", "origin", "main"]).status.success());
    let remote = String::from_utf8(git(&fixture.remote, &["rev-parse", "refs/heads/main"]).stdout)
        .expect("UTF-8 oid")
        .trim()
        .to_owned();
    git(&fixture.work, &["reset", "--hard", &base]);

    let server = IpcServer::start(fixture.socket_path(home), Arc::new(ConfirmingHandler))
        .expect("start the stub IPC server");
    let stdin = format!("refs/heads/main {base} refs/heads/main {remote}\n");
    let output = fixture.run_cli(
        home,
        &["hook", "pre-push", "origin", "file:///tmp/remote.git"],
        &stdin,
        &[],
    );

    assert_eq!(
        output.status.code(),
        Some(0),
        "确认后应固定退出码 0，实际: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("confirmed"));
    // CLI 只返回 0，不自己 push：远端必须仍停在确认时的 OID。
    assert_eq!(
        fixture.remote_oid("refs/heads/main"),
        format!("{remote}\n").as_bytes()
    );

    drop(server);
}

#[test]
fn confirmed_force_push_updates_the_remote() {
    // 端到端：真实 `git push --force` → Hook 请求确认 → 桩服务返回 Confirmed
    // → CLI 返回 0 → 原 git push 完成唯一一次推送。
    let fixture = GitFixture::new();
    let home_dir = short_isolated_home();
    let home = home_dir.path();
    fixture.write_config(home, true);

    // 先用调试模式建立远端基线，再让远端领先于本地。
    fixture.install_policy("allow");
    assert!(fixture.push(&["push", "origin", "main"]).status.success());
    let base = String::from_utf8(git(&fixture.work, &["rev-parse", "HEAD"]).stdout)
        .expect("UTF-8 oid")
        .trim()
        .to_owned();
    fixture.commit_change("remote only change\n");
    assert!(fixture.push(&["push", "origin", "main"]).status.success());
    git(&fixture.work, &["reset", "--hard", &base]);

    fixture.install_real_hook();
    let server = IpcServer::start(fixture.socket_path(home), Arc::new(ConfirmingHandler))
        .expect("start the stub IPC server");
    let output = fixture.push_with_home(home, &["push", "--force", "origin", "main"]);

    assert!(
        output.status.success(),
        "确认后的强制推送应成功: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // 推送由原 git push 完成：远端 ref 变为确认时的本地 OID。
    assert_eq!(
        fixture.remote_oid("refs/heads/main"),
        format!("{base}\n").as_bytes()
    );

    drop(server);
}
