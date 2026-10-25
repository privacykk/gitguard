//! `pre-push` Hook 的安装、卸载与健康检查。
//!
//! 依赖方向是 `gitguard-hook → gitguard-git / gitguard-config → gitguard-core`：
//! 找 Hook 目录需要执行 Git，所以走 `gitguard-git`；Hook 状态类型来自
//! `gitguard-config`。本 crate 不判断「该不该推送」，也不启动任何常驻进程。
//!
//! 安全约定：
//! - 只改写 GitGuard 自己安装的 Hook；用户原本的 Hook 会先备份为
//!   `pre-push.gitguard-backup`，脚本里再链式调用它，绝不直接丢弃。
//! - 脚本里的路径一律用单引号包裹（内部单引号会被转义），不把外部输入拼进 shell；
//!   路径含换行或 NUL 时直接报错，避免截断脚本。
//! - 脚本里没有任何「跳过确认」的开关：每次 push 都会先调用 CLI，CLI 以 0 放行后
//!   才链式执行被备份的原 Hook。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use gitguard_config::HookStatus;
use gitguard_core::{GitGuardError, Result};

/// Hook 文件名（Git 固定叫 `pre-push`）。
const HOOK_FILE_NAME: &str = "pre-push";

/// 备份文件后缀：拼在 Hook 文件名后面，与被备份的 Hook 放在同一目录。
const BACKUP_SUFFIX: &str = ".gitguard-backup";

/// 写入脚本第二行的标识。健康检查靠它判断「这个 Hook 由 GitGuard 管理」。
const HOOK_MARKER: &str = "# GitGuard pre-push hook - do not edit manually";

/// 脚本传给 CLI 的仓库 id 参数。
///
/// CLI 靠它把「当前仓库的 Hook」与「配置里的登记记录」对应起来：即使仓库目录被
/// 移动、配置文件被误删，也能看出这条 Hook 原本属于哪次登记，从而拒绝静默放行。
const REPOSITORY_ID_ARG: &str = "--repository-id";

/// 与某个仓库相关的三个路径，避免调用方自己拼文件名。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookPaths {
    /// Hook 目录，已考虑 `core.hooksPath`。
    pub hooks_dir: PathBuf,
    /// `pre-push` Hook 的完整路径。
    pub hook: PathBuf,
    /// 被备份的原始 Hook 路径；文件不存在说明没有需要链式调用的 Hook。
    pub backup: PathBuf,
}

/// 解析仓库的 Hook 路径（目录、Hook 文件、备份文件）。
///
/// 目录存在与否不影响结果；真正写文件时才创建目录。
pub fn hook_paths(repo_path: &Path) -> Result<HookPaths> {
    let hooks_dir = gitguard_git::hooks_path(repo_path)?;
    Ok(HookPaths {
        hook: hooks_dir.join(HOOK_FILE_NAME),
        backup: hooks_dir.join(format!("{HOOK_FILE_NAME}{BACKUP_SUFFIX}")),
        hooks_dir,
    })
}

/// 安装 GitGuard 的 `pre-push` Hook。
///
/// `repository_id` 是这条仓库记录在配置里的 id，会写进脚本传给 CLI，
/// 让 CLI 在「配置记录丢失」时仍能判断出这是一条已登记的 Hook。
///
/// 行为：
/// - Hook 不存在：直接写入 GitGuard 脚本；
/// - Hook 已是 GitGuard 管理：按当前二进制路径与 id 重写，不会重复备份；
/// - Hook 是用户的其它脚本：先备份为 `pre-push.gitguard-backup`，再写入链式调用脚本；
/// - Hook 是别人的脚本、但备份已存在：说明有人绕过 GitGuard 改了 Hook，
///   此时拒绝继续，避免把用户原有的 Hook 覆盖掉。
///
/// 写入采用「同目录临时文件 + rename」，避免中途失败留下半截脚本。
pub fn install_hook(repo_path: &Path, gitguard_binary: &Path, repository_id: &str) -> Result<()> {
    validate_repository_id(repository_id)?;

    let binary = shell_quote(gitguard_binary)?;
    let paths = hook_paths(repo_path)?;

    fs::create_dir_all(&paths.hooks_dir).map_err(|error| {
        GitGuardError::Io(format!(
            "could not create the hook directory {}: {error}",
            paths.hooks_dir.display()
        ))
    })?;

    let existing = read_existing_hook(&paths.hook)?;
    let backup_exists = paths.backup.exists();

    let script = match existing {
        Some(content) if is_gitguard_hook(&content) => {
            // 已由 GitGuard 管理：沿用是否备份过的事实，只刷新脚本内容。
            build_script(
                &binary,
                repository_id,
                backup_exists.then_some(&paths.backup),
            )?
        }
        Some(_) => {
            if backup_exists {
                return Err(GitGuardError::InvalidInput(format!(
                    "refusing to install: {} was modified outside GitGuard and {} already exists",
                    paths.hook.display(),
                    paths.backup.display()
                )));
            }
            fs::copy(&paths.hook, &paths.backup).map_err(|error| {
                GitGuardError::Io(format!(
                    "could not back up the existing hook {}: {error}",
                    paths.hook.display()
                ))
            })?;
            // 备份会被链式脚本直接执行，必须是可执行文件；
            // 某些工具写出的 Hook 会丢掉执行位，这里显式补上。
            set_executable(&paths.backup)?;
            build_script(&binary, repository_id, Some(&paths.backup))?
        }
        None => build_script(
            &binary,
            repository_id,
            backup_exists.then_some(&paths.backup),
        )?,
    };

    write_hook(&paths.hook, &script)
}

/// 卸载 GitGuard 的 `pre-push` Hook。
///
/// - 备份存在：把备份还原成 `pre-push`，用户原本的 Hook 继续生效；
/// - 没有备份：删除 GitGuard 写入的 Hook 文件；
/// - Hook 已不是 GitGuard 管理的（被其它工具替换）：不做任何删除，返回错误；
/// - Hook 与备份都不存在：视为已经卸载，直接成功。
pub fn uninstall_hook(repo_path: &Path) -> Result<()> {
    let paths = hook_paths(repo_path)?;
    let existing = read_existing_hook(&paths.hook)?;
    let backup_exists = paths.backup.exists();

    match existing {
        None => {
            // 只有备份时说明人工删掉了 Hook，仍然把用户的 Hook 放回去。
            if backup_exists {
                restore_backup(&paths.backup, &paths.hook)?;
            }
            Ok(())
        }
        Some(content) if is_gitguard_hook(&content) => {
            if backup_exists {
                fs::remove_file(&paths.hook).map_err(|error| {
                    GitGuardError::Io(format!(
                        "could not remove the GitGuard hook {}: {error}",
                        paths.hook.display()
                    ))
                })?;
                restore_backup(&paths.backup, &paths.hook)
            } else {
                fs::remove_file(&paths.hook).map_err(|error| {
                    GitGuardError::Io(format!(
                        "could not remove the GitGuard hook {}: {error}",
                        paths.hook.display()
                    ))
                })
            }
        }
        Some(_) => Err(GitGuardError::InvalidInput(format!(
            "refusing to uninstall: {} was not installed by GitGuard",
            paths.hook.display()
        ))),
    }
}

/// 检查 Hook 的安装状态。
///
/// 这里只读文件、不改动任何内容。无法解析 Hook 目录或读取失败时返回
/// [`HookStatus::Unknown`]，绝不猜测成「健康」。
///
/// 判定依据是脚本里的 [`HOOK_MARKER`]：
/// - 文件不存在 → [`HookStatus::Missing`]；
/// - 含标记 → [`HookStatus::Healthy`]（用户手工追加的行不会改变结论）；
/// - 不含标记且存在备份 → [`HookStatus::Modified`]；
/// - 不含标记且没有备份 → [`HookStatus::Conflict`]（用户的其它 Hook 占着位置）。
pub fn check_hook_health(repo_path: &Path) -> HookStatus {
    let Ok(paths) = hook_paths(repo_path) else {
        return HookStatus::Unknown;
    };

    match read_existing_hook(&paths.hook) {
        Ok(Some(content)) if is_gitguard_hook(&content) => HookStatus::Healthy,
        Ok(Some(_)) if paths.backup.exists() => HookStatus::Modified,
        Ok(Some(_)) => HookStatus::Conflict,
        Ok(None) => HookStatus::Missing,
        Err(_) => HookStatus::Unknown,
    }
}

/// 读取现有 Hook 的内容。
///
/// 文件不存在返回 `Ok(None)`；其它读取失败（权限、非法 UTF-8 等）返回错误，
/// 让调用方按「无法判断」处理，而不是当作没有 Hook。
fn read_existing_hook(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(GitGuardError::Io(format!(
            "could not read the hook {}: {error}",
            path.display()
        ))),
    }
}

/// 判断脚本是否由 GitGuard 写入：只认整行标记，避免用户注释里恰好包含同样文字。
fn is_gitguard_hook(content: &str) -> bool {
    content.lines().any(|line| line.trim() == HOOK_MARKER)
}

/// 生成 Hook 脚本。
///
/// `backup` 为 `Some` 时生成链式脚本：先问 GitGuard，GitGuard 以 0 放行后才执行
/// 用户原本的 Hook；GitGuard 拒绝时 `|| exit $?` 立即结束，不会绕到用户 Hook。
///
/// 脚本里不包含任何「跳过确认」的开关：放行只能来自 CLI 的确认流程。
/// 仓库 id 只用作「这条 Hook 属于哪次登记」的标记，不能改变 CLI 的判定结果。
///
/// 备份路径无法安全地写进 shell（非绝对路径、非 UTF-8、含换行等）时返回错误，
/// 而不是把可疑的字符串写进 Hook 脚本。
fn build_script(binary: &str, repository_id: &str, backup: Option<&Path>) -> Result<String> {
    match backup {
        None => Ok(format!(
            "#!/bin/sh\n{HOOK_MARKER}\nexec {binary} hook pre-push {REPOSITORY_ID_ARG} '{repository_id}' \"$@\"\n"
        )),
        Some(backup) => {
            let backup = shell_quote(backup)?;
            Ok(format!(
                "#!/bin/sh\n{HOOK_MARKER}\n\
                 {binary} hook pre-push {REPOSITORY_ID_ARG} '{repository_id}' \"$@\" || exit $?\n\
                 exec {backup} \"$@\"\n"
            ))
        }
    }
}

/// 校验写进脚本的仓库 id。
///
/// 这个值是外部传入的（来自配置），只能包含 UUID 字符集；否则一律拒绝，
/// 不让可疑字符串变成 Hook 脚本里的 shell 片段。
fn validate_repository_id(repository_id: &str) -> Result<()> {
    let valid = !repository_id.is_empty()
        && repository_id.len() <= 36
        && repository_id
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == '-');

    if valid {
        Ok(())
    } else {
        Err(GitGuardError::InvalidInput(
            "repository id must be a UUID before it can be written into a hook".into(),
        ))
    }
}

/// 把脚本原子地写到 Hook 路径，并赋予可执行权限。
fn write_hook(hook: &Path, script: &str) -> Result<()> {
    let dir = hook.parent().ok_or_else(|| {
        GitGuardError::Internal(format!(
            "hook path {} has no parent directory",
            hook.display()
        ))
    })?;

    // 临时文件与目标同目录，rename 才能一次替换；名字带后缀，不会被 Git 当成 Hook。
    let temp_path = dir.join(format!("{HOOK_FILE_NAME}.gitguard-tmp"));

    if let Err(error) = write_file(&temp_path, script) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    if let Err(error) = fs::rename(&temp_path, hook) {
        let _ = fs::remove_file(&temp_path);
        return Err(GitGuardError::Io(format!(
            "could not replace the hook {}: {error}",
            hook.display()
        )));
    }

    set_executable(hook)
}

/// 写文件并刷盘；失败时清理由调用方负责。
fn write_file(path: &Path, contents: &str) -> Result<()> {
    use std::io::Write;

    let mut file = fs::File::create(path).map_err(|error| {
        GitGuardError::Io(format!("could not create {}: {error}", path.display()))
    })?;
    file.write_all(contents.as_bytes()).map_err(|error| {
        GitGuardError::Io(format!("could not write {}: {error}", path.display()))
    })?;
    file.sync_all()
        .map_err(|error| GitGuardError::Io(format!("could not flush {}: {error}", path.display())))
}

/// 给 Hook 加上可执行权限；Git 只有在 Hook 可执行时才会调用它。
#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|error| {
        GitGuardError::Io(format!(
            "could not make the hook {} executable: {error}",
            path.display()
        ))
    })
}

/// 非 Unix 平台没有 POSIX 权限位，交给 Git for Windows 自行判断。
#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

/// 把备份还原成 Hook；rename 保留原文件的权限位。
fn restore_backup(backup: &Path, hook: &Path) -> Result<()> {
    fs::rename(backup, hook).map_err(|error| {
        GitGuardError::Io(format!(
            "could not restore the original hook {}: {error}",
            hook.display()
        ))
    })
}

/// 把路径转成 shell 单引号字面量。
///
/// 单引号字符串里没有转义序列，唯一需要处理的是 `'` 本身：
/// 写成 `'\''`（结束引号 → 转义的引号 → 重新开引号）。
/// 路径必须绝对、是合法 UTF-8，且不含换行或 NUL，否则直接报错而不是生成可疑脚本。
fn shell_quote(path: &Path) -> Result<String> {
    if !path.is_absolute() {
        return Err(GitGuardError::InvalidInput(format!(
            "path {} must be absolute before it can be written into a hook",
            path.display()
        )));
    }

    let text = path.to_str().ok_or_else(|| {
        GitGuardError::InvalidInput(format!("path {} is not valid UTF-8", path.display()))
    })?;

    if text.is_empty() || text.contains('\0') || text.contains('\n') || text.contains('\r') {
        return Err(GitGuardError::InvalidInput(format!(
            "path {} contains characters that cannot be used in a hook script",
            path.display()
        )));
    }

    Ok(format!("'{}'", text.replace('\'', r"'\''")))
}

// 测试只在 `tempfile` 临时仓库里运行真实 Git，不接触用户仓库与全局 Git 配置。
#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::process::Command;

    use tempfile::TempDir;

    use super::*;

    /// 一个临时 Git 仓库；`_temp` 只用于保持临时目录存活。
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
            Self { _temp: temp, root }
        }

        fn paths(&self) -> HookPaths {
            hook_paths(&self.root).expect("resolve hook paths")
        }

        /// 写入一个不属于 GitGuard 的 Hook，模拟用户已有的钩子。
        fn write_foreign_hook(&self, body: &str) -> PathBuf {
            let paths = self.paths();
            fs::create_dir_all(&paths.hooks_dir).expect("create hooks directory");
            fs::write(&paths.hook, body).expect("write foreign hook");
            paths.hook
        }
    }

    /// 运行 Git 并返回输出；隔离用户全局配置与交互提示。
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

    /// 用来验证「用户的 Hook 被链式调用」的哨兵：执行后写一个标记文件。
    fn sentinel_hook(marker: &Path) -> String {
        format!(
            "#!/bin/sh\n# user hook\necho ran >> '{}'\n",
            marker.display()
        )
    }

    fn sample_binary() -> PathBuf {
        PathBuf::from("/opt/gitguard/bin/gitguard")
    }

    /// 写进脚本的仓库 id；形状与配置里的 UUID 一致。
    fn sample_repository_id() -> &'static str {
        "00000000-0000-4000-8000-000000000000"
    }

    /// 一个总是以指定退出码结束的假 CLI，用来观察脚本的链式行为。
    fn stub_binary(dir: &Path, exit_code: i32) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = dir.join(format!("stub-gitguard-{exit_code}"));
        fs::write(&path, format!("#!/bin/sh\nexit {exit_code}\n")).expect("write stub cli");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("make stub cli executable");
        path
    }

    #[test]
    fn installs_a_hook_that_calls_the_cli_before_the_user_hook() {
        let repo = TestRepo::new();
        install_hook(&repo.root, &sample_binary(), sample_repository_id()).expect("install hook");

        let paths = repo.paths();
        let script = fs::read_to_string(&paths.hook).expect("read installed hook");
        assert!(script.starts_with("#!/bin/sh\n"));
        assert!(script.contains(HOOK_MARKER));
        assert!(script.contains("'/opt/gitguard/bin/gitguard' hook pre-push"));
        assert!(
            script.contains(&format!("{REPOSITORY_ID_ARG} '{}'", sample_repository_id())),
            "脚本必须把仓库 id 传给 CLI，否则记录丢失时 CLI 无法判断这条 Hook 属于哪次登记"
        );
        // 回归：脚本里不得再出现任何「跳过确认」的环境变量判断。
        assert!(!script.contains("GITGUARD_CONTROLLED_PUSH"));
        assert!(!paths.backup.exists(), "没有旧 Hook 时不应创建备份");
        assert_eq!(check_hook_health(&repo.root), HookStatus::Healthy);

        // 脚本必须是合法的 shell 语法，否则 Git 执行时会静默失败。
        let output = Command::new("sh")
            .arg("-n")
            .arg(&paths.hook)
            .output()
            .expect("run sh -n");
        assert!(
            output.status.success(),
            "generated hook is not valid shell: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn reinstalling_keeps_a_single_copy_without_backup() {
        let repo = TestRepo::new();
        install_hook(&repo.root, &sample_binary(), sample_repository_id()).expect("first install");
        let first = fs::read_to_string(repo.paths().hook).expect("read hook");
        install_hook(
            &repo.root,
            Path::new("/usr/local/bin/gitguard"),
            sample_repository_id(),
        )
        .expect("second install");

        let paths = repo.paths();
        let second = fs::read_to_string(&paths.hook).expect("read hook again");
        assert_ne!(first, second, "重新安装应刷新二进制路径");
        assert!(second.contains("'/usr/local/bin/gitguard'"));
        assert!(
            !paths.backup.exists(),
            "GitGuard 自己的 Hook 不能被当成旧 Hook 备份"
        );
        assert!(
            !paths.hooks_dir.join("pre-push.gitguard-tmp").exists(),
            "Hook 目录里不应残留临时文件"
        );
    }

    #[test]
    fn reinstall_updates_the_repository_id_in_the_script() {
        let repo = TestRepo::new();
        install_hook(&repo.root, &sample_binary(), sample_repository_id()).expect("first install");
        install_hook(
            &repo.root,
            &sample_binary(),
            "11111111-1111-4111-8111-111111111111",
        )
        .expect("second install");

        let script = fs::read_to_string(repo.paths().hook).expect("read hook");
        assert!(script.contains("'11111111-1111-4111-8111-111111111111'"));
        assert!(
            !script.contains(sample_repository_id()),
            "重写后不得残留旧 id，否则 CLI 会对着错误的记录判断"
        );
    }

    #[test]
    fn install_rejects_a_repository_id_that_could_escape_the_script() {
        let repo = TestRepo::new();
        for bad in [
            "",
            "not-a-uuid",
            "00000000-0000-4000-8000-00000000'; rm -rf /",
            "id\nline",
        ] {
            assert!(
                matches!(
                    install_hook(&repo.root, &sample_binary(), bad),
                    Err(GitGuardError::InvalidInput(_))
                ),
                "{bad:?} 不得被写进 Hook 脚本"
            );
        }
        assert!(!repo.paths().hook.exists(), "非法 id 不得写出任何 Hook");
    }

    #[test]
    fn existing_user_hook_is_backed_up_and_chained() {
        let repo = TestRepo::new();
        let marker = repo.root.join("user-hook-ran.txt");
        let original = sentinel_hook(&marker);
        repo.write_foreign_hook(&original);

        let cli = stub_binary(repo._temp.path(), 0);
        install_hook(&repo.root, &cli, sample_repository_id()).expect("install hook");

        let paths = repo.paths();
        assert_eq!(
            fs::read_to_string(&paths.backup).expect("read backup"),
            original,
            "备份必须与用户原本的 Hook 完全一致"
        );

        let script = fs::read_to_string(&paths.hook).expect("read hooked script");
        assert!(script.contains(HOOK_MARKER));
        assert!(script.contains(&format!("'{}' hook pre-push", cli.display())));
        assert!(script.contains("pre-push.gitguard-backup"));
        assert_eq!(check_hook_health(&repo.root), HookStatus::Healthy);

        // CLI 以 0 放行后才执行用户原本的 Hook。
        let status = Command::new("sh")
            .arg(&paths.hook)
            .status()
            .expect("run hook");
        assert!(status.success());
        assert!(marker.exists(), "CLI 放行后必须执行用户原本的 Hook");
    }

    #[test]
    fn a_rejected_confirmation_does_not_run_the_backed_up_hook() {
        // 安全关键：`|| exit $?` 必须在确认失败时立即结束，不能绕到用户 Hook。
        let repo = TestRepo::new();
        let marker = repo.root.join("user-hook-ran.txt");
        repo.write_foreign_hook(&sentinel_hook(&marker));
        let cli = stub_binary(repo._temp.path(), 1);
        install_hook(&repo.root, &cli, sample_repository_id()).expect("install hook");

        let status = Command::new("sh")
            .arg(repo.paths().hook)
            .status()
            .expect("run hook");

        assert_eq!(status.code(), Some(1));
        assert!(!marker.exists(), "确认失败时不得执行用户原本的 Hook");
    }

    #[test]
    fn install_refuses_to_clobber_a_foreign_hook_when_a_backup_exists() {
        let repo = TestRepo::new();
        repo.write_foreign_hook(&sentinel_hook(&repo.root.join("first.txt")));
        install_hook(&repo.root, &sample_binary(), sample_repository_id())
            .expect("first install backs up");

        // 模拟外部工具覆盖了 GitGuard 的 Hook，但备份还在：不能再次备份覆盖旧备份。
        let paths = repo.paths();
        fs::write(&paths.hook, "#!/bin/sh\necho replaced\n").expect("replace hook");

        assert!(matches!(
            install_hook(&repo.root, &sample_binary(), sample_repository_id()),
            Err(GitGuardError::InvalidInput(_))
        ));
        assert!(
            fs::read_to_string(&paths.backup)
                .expect("backup should still exist")
                .contains("user hook"),
            "旧备份不能被覆盖"
        );
    }

    #[test]
    fn health_distinguishes_missing_healthy_modified_and_conflict() {
        let repo = TestRepo::new();
        assert_eq!(check_hook_health(&repo.root), HookStatus::Missing);

        // 用户自己的 Hook，且没有备份：冲突。
        repo.write_foreign_hook("#!/bin/sh\nexit 0\n");
        assert_eq!(check_hook_health(&repo.root), HookStatus::Conflict);

        // 有备份但当前 Hook 不是 GitGuard 写的：视为被修改过。
        install_hook(&repo.root, &sample_binary(), sample_repository_id()).expect("install hook");
        let paths = repo.paths();
        fs::write(&paths.hook, "#!/bin/sh\nexit 0\n").expect("rewrite hook");
        assert_eq!(check_hook_health(&repo.root), HookStatus::Modified);

        // 非 Git 仓库无法解析 Hook 目录：只能是 Unknown。
        let temp = tempfile::tempdir().expect("create temporary directory");
        assert_eq!(check_hook_health(temp.path()), HookStatus::Unknown);
    }

    #[test]
    fn uninstall_removes_a_plain_hook_and_is_idempotent() {
        let repo = TestRepo::new();
        install_hook(&repo.root, &sample_binary(), sample_repository_id()).expect("install hook");

        uninstall_hook(&repo.root).expect("uninstall hook");
        let paths = repo.paths();
        assert!(!paths.hook.exists(), "没有备份时应删除 Hook 文件");
        assert!(!paths.backup.exists());

        uninstall_hook(&repo.root).expect("uninstall should be idempotent");
        assert_eq!(check_hook_health(&repo.root), HookStatus::Missing);
    }

    #[test]
    fn uninstall_restores_the_original_hook() {
        let repo = TestRepo::new();
        let original = sentinel_hook(&repo.root.join("user-hook-ran.txt"));
        repo.write_foreign_hook(&original);
        install_hook(&repo.root, &sample_binary(), sample_repository_id()).expect("install hook");

        uninstall_hook(&repo.root).expect("uninstall hook");

        let paths = repo.paths();
        assert_eq!(
            fs::read_to_string(&paths.hook).expect("read restored hook"),
            original,
            "卸载后必须还原用户原本的 Hook"
        );
        assert!(!paths.backup.exists(), "还原之后备份不应继续存在");
    }

    #[test]
    fn uninstall_restores_the_backup_when_the_hook_was_deleted_manually() {
        let repo = TestRepo::new();
        let original = sentinel_hook(&repo.root.join("user-hook-ran.txt"));
        repo.write_foreign_hook(&original);
        install_hook(&repo.root, &sample_binary(), sample_repository_id()).expect("install hook");
        fs::remove_file(repo.paths().hook).expect("delete hook manually");

        uninstall_hook(&repo.root).expect("uninstall restores backup");
        assert_eq!(
            fs::read_to_string(repo.paths().hook).expect("read restored hook"),
            original
        );
    }

    #[test]
    fn uninstall_never_removes_a_hook_gitguard_did_not_install() {
        let repo = TestRepo::new();
        let foreign = "#!/bin/sh\n# user hook\nexit 0\n";
        repo.write_foreign_hook(foreign);

        assert!(matches!(
            uninstall_hook(&repo.root),
            Err(GitGuardError::InvalidInput(_))
        ));
        assert_eq!(
            fs::read_to_string(repo.paths().hook).expect("read hook"),
            foreign,
            "用户的 Hook 必须原样保留"
        );
    }

    #[test]
    fn shell_quote_escapes_single_quotes_and_rejects_unsafe_paths() {
        assert_eq!(
            shell_quote(Path::new("/tmp/it's here/gitguard")).expect("quote path"),
            r"'/tmp/it'\''s here/gitguard'"
        );
        assert!(matches!(
            shell_quote(Path::new("relative/gitguard")),
            Err(GitGuardError::InvalidInput(_))
        ));
        assert!(matches!(
            shell_quote(Path::new("/tmp/bad\npath")),
            Err(GitGuardError::InvalidInput(_))
        ));
    }

    #[test]
    fn build_script_reports_a_backup_path_it_cannot_quote() {
        // 回归：无法安全引用的备份路径曾被静默替换成 `''`，于是 Hook 里留下一条
        // 空命令，用户原本的 Hook 被悄无声息地丢掉。
        let error = build_script(
            "'/opt/gitguard/bin/gitguard'",
            sample_repository_id(),
            Some(Path::new("relative/pre-push")),
        )
        .expect_err("relative backup path must not reach the script");
        assert!(matches!(error, GitGuardError::InvalidInput(_)));
    }

    #[test]
    fn hook_paths_use_the_repository_hooks_directory() {
        let repo = TestRepo::new();
        let paths = repo.paths();

        assert!(paths.hooks_dir.is_absolute());
        assert_eq!(paths.hook, paths.hooks_dir.join(HOOK_FILE_NAME));
        assert_eq!(
            paths.backup,
            paths
                .hooks_dir
                .join(format!("{HOOK_FILE_NAME}{BACKUP_SUFFIX}"))
        );
    }
}
