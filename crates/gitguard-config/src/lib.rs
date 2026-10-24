//! 本地 JSON 配置层：保存应用级设置，以及「哪些仓库启用了推送确认」的登记表。
//!
//! 这一层只做一件事：把结构体安全地写到磁盘、再完整读回来。
//! 它不做策略判断、不弹窗、不执行 Git 命令，也不访问网络；
//! 依赖方向是 `gitguard-config → gitguard-core`。
//!
//! 配置文件位置由 [`config_dir`] 计算：
//! - macOS：`~/Library/Application Support/dev.gitguard.desktop/`
//! - Linux：`$XDG_CONFIG_HOME/gitguard/`（未设置 `XDG_CONFIG_HOME` 时是 `~/.config/gitguard/`）
//! - Windows：`%APPDATA%\gitguard\`
//!
//! 注意：配置缺失或损坏时本层只负责如实返回默认值或错误，
//! 真正的「是否放行 push」由 Rust 策略层决定，界面与本层都不是安全边界。

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use gitguard_core::{GitGuardError, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 配置文件格式版本。只有在结构发生不兼容变化时才递增。
pub const SCHEMA_VERSION: u32 = 1;

/// 配置文件名，固定放在 [`config_dir`] 目录下。
const CONFIG_FILE_NAME: &str = "config.json";

/// 写入时使用的临时文件名；与正式文件同目录，才能用一次 rename 完成替换。
const TEMP_FILE_NAME: &str = ".config.json.tmp";

/// 整个应用的配置根对象，对应磁盘上的 `config.json`。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppConfig {
    /// 配置格式版本，目前固定为 [`SCHEMA_VERSION`]。
    pub schema_version: u32,
    /// 所有仓库共用的默认设置。
    pub global: GlobalConfig,
    /// 已登记的仓库列表；没有登记的仓库使用 `global` 里的默认值。
    pub repositories: Vec<RepoConfig>,
}

/// 全局默认设置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlobalConfig {
    /// 未单独配置的仓库默认是否需要推送确认。
    pub default_push_confirmation: bool,
    /// 等待用户确认的超时时间（秒），超时按拒绝处理。
    pub confirmation_timeout_seconds: u64,
}

/// 单个仓库的配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoConfig {
    /// 仓库的稳定标识（UUID v4）。仓库被移动或改名时它不变。
    pub repository_id: String,
    /// 仓库的规范路径，用于把用户的目录匹配到这条记录。
    pub canonical_path: String,
    /// 这个仓库是否启用推送确认；覆盖 `global.default_push_confirmation`。
    pub push_confirmation_enabled: bool,
    /// Hook 的安装状态；这里只保存状态，不执行任何安装或修复动作。
    pub hook_status: HookStatus,
}

/// `pre-push` Hook 的安装状态。
///
/// 未知或无法判断的情况用 [`HookStatus::Unknown`] 表示，不要猜测成 `Healthy`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HookStatus {
    /// Hook 已安装且内容与当前版本一致。
    Healthy,
    /// 仓库里没有安装 Hook。
    Missing,
    /// Hook 存在但内容被修改过。
    Modified,
    /// Hook 存在但与用户的其它 Hook 管理器冲突。
    Conflict,
    /// 尚未检查，或检查过程中无法判断。
    Unknown,
}

impl Default for AppConfig {
    /// 首次运行时的默认配置：一个空的仓库列表加默认全局设置。
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            global: GlobalConfig::default(),
            repositories: Vec::new(),
        }
    }
}

impl Default for GlobalConfig {
    /// 默认要求推送确认，超时 180 秒。
    fn default() -> Self {
        Self {
            default_push_confirmation: true,
            confirmation_timeout_seconds: 180,
        }
    }
}

/// 返回本应用的配置目录；这里只计算路径，不会创建目录。
///
/// 由 `dirs::config_dir()` 提供平台基准目录，再加上应用专属子目录，
/// 避免把配置直接散落在用户主目录里。
pub fn config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().ok_or_else(|| {
        GitGuardError::Internal("could not determine the platform configuration directory".into())
    })?;

    // macOS 的基准目录是 `~/Library/Application Support`，按 macOS 习惯用反向域名；
    // Linux / Windows 的基准目录已经是用户配置目录，直接追加应用名即可。
    let app_dir = if cfg!(target_os = "macos") {
        base.join("dev.gitguard.desktop")
    } else {
        base.join("gitguard")
    };

    Ok(app_dir)
}

/// 返回配置文件的完整路径（`<配置目录>/config.json`）。
pub fn config_file_path() -> Result<PathBuf> {
    Ok(config_dir()?.join(CONFIG_FILE_NAME))
}

/// 从默认位置加载配置。
///
/// - 文件不存在：返回默认配置（首次运行属正常情况）；
/// - 文件内容不是合法 JSON 或版本不认识：返回 [`GitGuardError::InvalidInput`]，
///   绝不悄悄用默认值覆盖用户的文件。
pub fn load_config() -> Result<AppConfig> {
    load_config_from(&config_file_path()?)
}

/// 把配置写入默认位置。
///
/// 采用「先写同目录临时文件、再 rename 覆盖」的原子写法，
/// 避免写到一半崩溃时留下半截 JSON 把原配置破坏掉。
pub fn save_config(config: &AppConfig) -> Result<()> {
    save_config_to(config, &config_file_path()?)
}

/// 按规范路径查找仓库配置（只读）。
pub fn find_repo<'a>(config: &'a AppConfig, canonical_path: &str) -> Option<&'a RepoConfig> {
    config
        .repositories
        .iter()
        .find(|repo| repo.canonical_path == canonical_path)
}

/// 按规范路径查找仓库配置（可修改），用于更新 Hook 状态等字段。
pub fn find_repo_mut<'a>(
    config: &'a mut AppConfig,
    canonical_path: &str,
) -> Option<&'a mut RepoConfig> {
    config
        .repositories
        .iter_mut()
        .find(|repo| repo.canonical_path == canonical_path)
}

/// 登记或更新一个仓库，并返回它的最新配置。
///
/// - 已登记：只更新 `push_confirmation_enabled`，保留原有 `repository_id` 与 Hook 状态；
/// - 未登记：新建一条记录，分配新的 UUID，Hook 状态记为 [`HookStatus::Unknown`]。
///
/// 调用方修改完返回值后，需要再调用 [`save_config`] 才会落到磁盘。
pub fn add_or_update_repo(
    config: &mut AppConfig,
    canonical_path: String,
    enabled: bool,
) -> &mut RepoConfig {
    let existing = config
        .repositories
        .iter()
        .position(|repo| repo.canonical_path == canonical_path);

    let index = match existing {
        Some(index) => {
            // 已登记过：沿用原记录，避免同一路径出现重复条目。
            config.repositories[index].push_confirmation_enabled = enabled;
            index
        }
        None => {
            config.repositories.push(RepoConfig {
                repository_id: Uuid::new_v4().to_string(),
                canonical_path,
                push_confirmation_enabled: enabled,
                hook_status: HookStatus::Unknown,
            });
            // 刚 push 进去的元素一定在最后一位。
            config.repositories.len() - 1
        }
    };

    &mut config.repositories[index]
}

/// 判断某个仓库当前是否需要推送确认。
///
/// 仓库自己的设置优先；没有登记过的仓库回落到全局默认值。
/// 默认值是「需要确认」，因此未登记的仓库不会因为缺少配置而被静默放行。
pub fn is_push_confirmation_enabled(config: &AppConfig, repo_path: &str) -> bool {
    match find_repo(config, repo_path) {
        Some(repo) => repo.push_confirmation_enabled,
        None => config.global.default_push_confirmation,
    }
}

/// 从指定文件加载配置，供默认位置、桌面端与测试共用。
pub fn load_config_from(path: &Path) -> Result<AppConfig> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        // 文件不存在说明还没保存过配置，用默认值即可，不算错误。
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(AppConfig::default()),
        Err(error) => {
            return Err(GitGuardError::Io(format!(
                "could not read config file {}: {error}",
                path.display()
            )));
        }
    };

    let config: AppConfig = serde_json::from_slice(&bytes).map_err(|error| {
        GitGuardError::InvalidInput(format!(
            "config file {} is not valid JSON: {error}",
            path.display()
        ))
    })?;

    // 版本不认识时直接报错：若照旧读入再写回，可能把新版本里的字段丢掉。
    if config.schema_version != SCHEMA_VERSION {
        return Err(GitGuardError::InvalidInput(format!(
            "unsupported config schema version {} in {} (expected {})",
            config.schema_version,
            path.display(),
            SCHEMA_VERSION
        )));
    }

    Ok(config)
}

/// 把配置写到指定文件，供默认位置、桌面端与测试共用。
pub fn save_config_to(config: &AppConfig, path: &Path) -> Result<()> {
    let dir = path.parent().ok_or_else(|| {
        GitGuardError::Internal(format!(
            "config path {} has no parent directory",
            path.display()
        ))
    })?;

    fs::create_dir_all(dir).map_err(|error| {
        GitGuardError::Io(format!(
            "could not create config directory {}: {error}",
            dir.display()
        ))
    })?;

    let json = serde_json::to_vec_pretty(config).map_err(|error| {
        GitGuardError::Internal(format!("could not serialize config to JSON: {error}"))
    })?;

    // 临时文件必须和正式文件在同一分区，rename 才能是原子替换。
    let temp_path = dir.join(TEMP_FILE_NAME);

    if let Err(error) = write_file(&temp_path, &json) {
        // 写失败时清掉残留的临时文件，避免下次误读半截内容。
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    if let Err(error) = fs::rename(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(GitGuardError::Io(format!(
            "could not replace config file {}: {error}",
            path.display()
        )));
    }

    Ok(())
}

/// 一次写出完整内容并刷盘；失败时把底层 IO 错误转成 [`GitGuardError::Io`]。
fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = fs::File::create(path).map_err(|error| {
        GitGuardError::Io(format!("could not create {}: {error}", path.display()))
    })?;

    file.write_all(bytes).map_err(|error| {
        GitGuardError::Io(format!("could not write {}: {error}", path.display()))
    })?;

    // 先落盘再 rename，降低掉电或崩溃后拿到空文件的概率。
    file.sync_all().map_err(|error| {
        GitGuardError::Io(format!("could not flush {}: {error}", path.display()))
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 在临时目录里拼出与真实配置同名的文件路径。
    /// 测试绝不触碰用户真实的配置目录。
    fn temp_config_path(dir: &Path) -> PathBuf {
        dir.join(CONFIG_FILE_NAME)
    }

    fn tempdir() -> tempfile::TempDir {
        tempfile::tempdir().expect("temporary directory should be created")
    }

    #[test]
    fn default_config_has_expected_values() {
        let config = AppConfig::default();
        assert_eq!(config.schema_version, SCHEMA_VERSION);
        assert!(config.global.default_push_confirmation);
        assert_eq!(config.global.confirmation_timeout_seconds, 180);
        assert!(config.repositories.is_empty());
    }

    #[test]
    fn hooks_status_serializes_as_plain_strings() {
        // JSON 里必须是可读的字符串，方便人工排查配置文件。
        let json = serde_json::to_string(&HookStatus::Modified).expect("status should serialize");
        assert_eq!(json, "\"Modified\"");
        let restored: HookStatus =
            serde_json::from_str(&json).expect("status should deserialize back");
        assert_eq!(restored, HookStatus::Modified);
    }

    #[test]
    fn config_round_trips_through_json() {
        let mut config = AppConfig {
            global: GlobalConfig {
                confirmation_timeout_seconds: 60,
                ..GlobalConfig::default()
            },
            ..AppConfig::default()
        };
        add_or_update_repo(&mut config, "/Users/dev/project".into(), false);

        let json = serde_json::to_string(&config).expect("config should serialize");
        let restored: AppConfig = serde_json::from_str(&json).expect("json should deserialize");

        assert_eq!(restored, config);
    }

    #[test]
    fn find_repo_locates_by_canonical_path() {
        let mut config = AppConfig::default();
        add_or_update_repo(&mut config, "/repos/a".into(), true);
        add_or_update_repo(&mut config, "/repos/b".into(), false);

        assert!(find_repo(&config, "/repos/a").is_some());
        assert_eq!(
            find_repo(&config, "/repos/b").map(|repo| repo.push_confirmation_enabled),
            Some(false)
        );
        assert!(find_repo(&config, "/repos/missing").is_none());
    }

    #[test]
    fn find_repo_mut_updates_in_place() {
        let mut config = AppConfig::default();
        add_or_update_repo(&mut config, "/repos/a".into(), true);

        let repo = find_repo_mut(&mut config, "/repos/a").expect("repo should exist");
        repo.hook_status = HookStatus::Healthy;
        repo.push_confirmation_enabled = false;

        let stored = find_repo(&config, "/repos/a").expect("repo should still exist");
        assert_eq!(stored.hook_status, HookStatus::Healthy);
        assert!(!stored.push_confirmation_enabled);
        assert!(find_repo_mut(&mut config, "/repos/missing").is_none());
    }

    #[test]
    fn add_or_update_repo_creates_then_updates() {
        let mut config = AppConfig::default();

        let created_id = add_or_update_repo(&mut config, "/repos/a".into(), true)
            .repository_id
            .clone();
        assert_eq!(config.repositories.len(), 1);
        assert!(
            Uuid::parse_str(&created_id).is_ok(),
            "repository id should be a UUID, got {created_id}"
        );
        assert_eq!(config.repositories[0].hook_status, HookStatus::Unknown);

        let updated = add_or_update_repo(&mut config, "/repos/a".into(), false);
        assert_eq!(updated.repository_id, created_id, "更新时 ID 必须保持不变");
        assert!(!updated.push_confirmation_enabled);
        assert_eq!(config.repositories.len(), 1, "同一路径不能重复登记");
    }

    #[test]
    fn repo_setting_overrides_global_default() {
        let mut config = AppConfig::default(); // 全局默认要求确认
        add_or_update_repo(&mut config, "/repos/off".into(), false);

        assert!(
            !is_push_confirmation_enabled(&config, "/repos/off"),
            "仓库级关闭必须覆盖全局默认"
        );
        assert!(
            is_push_confirmation_enabled(&config, "/repos/unregistered"),
            "未登记的仓库应回落到全局默认（true）"
        );

        config.global.default_push_confirmation = false;
        assert!(!is_push_confirmation_enabled(
            &config,
            "/repos/unregistered"
        ));

        add_or_update_repo(&mut config, "/repos/on".into(), true);
        assert!(is_push_confirmation_enabled(&config, "/repos/on"));
    }

    #[test]
    fn config_file_path_lives_in_config_dir() {
        let dir = config_dir().expect("platform config dir should be resolvable");
        let file = config_file_path().expect("config file path should be resolvable");

        assert!(file.is_absolute());
        assert_eq!(Some(dir.as_path()), file.parent());
        assert_eq!(
            file.file_name().and_then(|name| name.to_str()),
            Some(CONFIG_FILE_NAME)
        );

        let expected_dir_name = if cfg!(target_os = "macos") {
            "dev.gitguard.desktop"
        } else {
            "gitguard"
        };
        assert_eq!(
            dir.file_name().and_then(|name| name.to_str()),
            Some(expected_dir_name)
        );
    }

    #[test]
    fn load_config_returns_default_when_file_is_missing() {
        let dir = tempdir();
        let path = temp_config_path(dir.path());
        assert!(!path.exists(), "测试前提：文件不存在");

        let config = load_config_from(&path).expect("缺失文件应回落到默认配置");
        assert_eq!(config, AppConfig::default());
        assert!(!path.exists(), "加载缺失文件不得创建文件");
    }

    #[test]
    fn save_config_then_load_config_round_trips() {
        let dir = tempdir();
        let path = temp_config_path(dir.path());

        let mut config = AppConfig {
            global: GlobalConfig {
                confirmation_timeout_seconds: 42,
                ..GlobalConfig::default()
            },
            ..AppConfig::default()
        };
        add_or_update_repo(&mut config, "/repos/a".into(), true);

        save_config_to(&config, &path).expect("保存应成功");
        assert!(path.exists(), "正式文件应已写入");

        let loaded = load_config_from(&path).expect("刚保存的配置应能加载");
        assert_eq!(loaded, config);
    }

    #[test]
    fn save_config_is_atomic_and_leaves_no_temporary_file() {
        let dir = tempdir();
        let path = temp_config_path(dir.path());

        save_config_to(&AppConfig::default(), &path).expect("第一次保存应成功");
        // 第二次保存会覆盖已存在的文件，同样不能留下临时文件。
        save_config_to(&AppConfig::default(), &path).expect("覆盖保存应成功");

        assert!(
            !dir.path().join(TEMP_FILE_NAME).exists(),
            "rename 之后临时文件必须消失"
        );

        let mut names: Vec<String> = fs::read_dir(dir.path())
            .expect("应能列出临时目录")
            .map(|entry| {
                entry
                    .expect("目录项应可读取")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        assert_eq!(names, vec![CONFIG_FILE_NAME.to_string()]);
    }

    #[test]
    fn save_config_creates_missing_directory() {
        let dir = tempdir();
        // 使用两层尚不存在的子目录，验证 create_dir_all 的行为。
        let path = dir
            .path()
            .join("nested")
            .join("config")
            .join(CONFIG_FILE_NAME);

        save_config_to(&AppConfig::default(), &path).expect("应自动创建目录并保存");
        assert!(path.exists());
    }

    #[test]
    fn malformed_config_file_is_rejected() {
        let dir = tempdir();
        let path = temp_config_path(dir.path());
        fs::write(&path, b"{ this is not json }").expect("应能写入测试文件");

        assert!(
            matches!(load_config_from(&path), Err(GitGuardError::InvalidInput(_))),
            "损坏的 JSON 必须报 InvalidInput，不能用默认值静默覆盖"
        );
    }

    #[test]
    fn unsupported_schema_version_is_rejected() {
        let dir = tempdir();
        let path = temp_config_path(dir.path());

        let config = AppConfig {
            schema_version: SCHEMA_VERSION + 1,
            ..AppConfig::default()
        };
        save_config_to(&config, &path).expect("保存应成功");

        assert!(
            matches!(load_config_from(&path), Err(GitGuardError::InvalidInput(_))),
            "版本不认识时应报 InvalidInput"
        );
    }
}
