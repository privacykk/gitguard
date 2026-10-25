//! Tauri 桌面容器入口、IPC 服务与命令层。
//!
//! 一次推送确认的完整链路：
//! 1. Git 触发 `pre-push` Hook → `gitguard` CLI；
//! 2. 没有控制终端时（SourceTree、VS Code 等图形客户端），CLI 通过 Unix Socket
//!    把 [`PushConfirmationRequest`] 发到本进程；
//! 3. 本层用 `gitguard-git` 列出待推送提交，保存为「待确认请求」并通知前端；
//! 4. 用户在界面上输入 yes 确认（或取消/超时）；
//! 5. 确认时重新校验 ref 未变化后回 `Confirmed`，由 Hook 放行、原始 `git push`
//!    自己把数据推上去；本进程不执行任何推送。
//!
//! 依赖方向是 `桌面端 → {ipc, config, git, hook} → core`：这里不判断策略，
//! 也不把「隐藏按钮」当作安全控制——CLI 只认 IPC 返回的 `Confirmed`，
//! 其余情况（含本进程未运行）一律拒绝推送。

mod menu;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gitguard_config::{AppConfig, HookStatus};
use gitguard_core::{
    CommitInfo, ConfirmationDecision, GitGuardError, PushConfirmationRequest,
    PushConfirmationResponse, Result,
};
use gitguard_ipc::{IpcHandler, IpcServer};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, UserAttentionType};
use uuid::Uuid;

/// 通知前端「有新的待确认请求」的 Tauri 事件名。
const EVENT_PUSH_REQUEST_RECEIVED: &str = "push-request-received";

/// 主窗口标签，与 `tauri.conf.json` 中的 `app.windows[0].label` 一致。
const MAIN_WINDOW_LABEL: &str = "main";

/// 指定 CLI 二进制路径的环境变量；用于开发与打包后 CLI 不在应用同级目录的场景。
const CLI_PATH_ENV: &str = "GITGUARD_CLI_PATH";

/// CLI 二进制名；Hook 脚本里调用的就是它。
const CLI_BINARY_NAME: &str = "gitguard";

/// 前端可翻译的稳定错误码。命令只返回这些码，不返回面向用户的句子。
const ERROR_CONFIGURATION_UNREADABLE: &str = "configuration_unreadable";
const ERROR_CONFIGURATION_WRITE_FAILED: &str = "configuration_write_failed";
const ERROR_REPOSITORY_NOT_FOUND: &str = "repository_not_found";
const ERROR_HOOK_INSTALL_FAILED: &str = "hook_install_failed";
const ERROR_CLI_NOT_FOUND: &str = "cli_binary_not_found";
const ERROR_NOT_A_REPOSITORY: &str = "not_a_repository";
/// 用户填写的路径不是绝对路径：相对路径会按桌面端进程的当前目录解析，结果不可预期。
const ERROR_PATH_NOT_ABSOLUTE: &str = "path_not_absolute";
const ERROR_NO_PENDING_REQUEST: &str = "no_pending_request";
const ERROR_REQUEST_INVALID: &str = "request_invalid";
const ERROR_INVALID_TIMEOUT: &str = "invalid_timeout";
const ERROR_IPC_UNAVAILABLE: &str = "ipc_unavailable";
const ERROR_INTERNAL: &str = "internal_error";

/// 允许的最小确认超时；0 秒等于立刻失败，对用户没有意义。
const MIN_TIMEOUT_SECONDS: u64 = 1;

/// 允许的最大确认超时（1 小时），避免配置写错导致 Hook 长时间挂住。
const MAX_TIMEOUT_SECONDS: u64 = 3600;

/// 桌面端比 CLI 更早结束等待的余量。
///
/// CLI 的 socket 读超时在「发出请求之前」就开始计时，桌面端从「收到请求时」才开始，
/// 两者相差一次连接与写入。留出这段余量后，桌面端就不会在 CLI 已经放弃之后才回
/// `Confirmed`——否则界面显示「已确认」，实际推送已被 Hook 拒绝。
const CONFIRMATION_DEADLINE_MARGIN: Duration = Duration::from_millis(250);

/// 桌面端等待时长的下限，避免极短超时被余量削成 0。
const MIN_CONFIRMATION_WAIT: Duration = Duration::from_millis(500);

/// 命令失败时返回给前端的结构：只有一个稳定错误码。
#[derive(Debug, Clone, Copy, Serialize)]
struct CommandError {
    code: &'static str,
}

fn command_error(code: &'static str) -> CommandError {
    CommandError { code }
}

/// 待确认推送的展示数据（前端 `PushConfirmation.vue` 使用）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PushConfirmationView {
    request_id: String,
    repository_path: String,
    remote_name: String,
    remote_url: String,
    /// 与 Hook stdin 的 ref 更新一一对应。
    refs: Vec<RefPreviewView>,
    /// 等待确认的秒数，前端据此显示倒计时。
    timeout_seconds: u64,
    /// 本进程收到请求的时间（Unix 秒），用于计算剩余时间。
    received_at_epoch_secs: u64,
}

/// 单个 ref 的展示数据。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RefPreviewView {
    local_ref: String,
    remote_ref: String,
    /// `create` / `update` / `delete`；前端据此提示风险，不参与任何判定。
    kind: &'static str,
    commits: Vec<CommitInfo>,
}

/// 设置页需要的全部数据，一次命令取回，避免前端多次往返。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsView {
    global: GlobalSettingsView,
    repositories: Vec<RepositoryView>,
    /// 配置文件损坏等问题的稳定错误码；有值时禁止写盘。
    config_error: Option<&'static str>,
    /// IPC 服务未能启动的错误码；有值时 Hook 一定连不上，推送会被拒绝。
    ipc_error: Option<&'static str>,
    /// 配置文件路径；设置页只读展示，前端不拼接也不猜测。
    config_path: String,
}

/// 全局设置：只影响未登记的仓库。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct GlobalSettingsView {
    /// 未登记仓库是否需要推送确认；登记过的仓库各看自己的开关。
    require_confirmation_for_unregistered_repositories: bool,
    confirmation_timeout_seconds: u64,
}

/// 仓库列表项。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryView {
    repository_id: String,
    canonical_path: String,
    push_confirmation_enabled: bool,
    hook_status: HookStatus,
}

/// 等待用户决定的推送请求；确认/取消时通过 `decision_tx` 唤醒 IPC 线程。
struct PendingPush {
    request: PushConfirmationRequest,
    view: PushConfirmationView,
    decision_tx: mpsc::Sender<ConfirmationDecision>,
    /// 桌面端自己的确认截止时间，早于 CLI 的等待上限。
    deadline: Instant,
}

/// 用户确认失败的原因，供命令层映射成稳定错误码。
#[derive(Debug)]
enum ConfirmationError {
    /// 请求已超时、已取消，或根本不存在。
    NoPendingRequest,
    /// 确认后 ref 已经变化，不能再放行。
    RefChanged(GitGuardError),
}

/// 只用于写 stderr 日志；程序行为不依赖这段文本。
impl std::fmt::Display for ConfirmationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoPendingRequest => {
                write!(formatter, "the confirmation request is no longer pending")
            }
            Self::RefChanged(error) => write!(formatter, "refs changed before the push: {error}"),
        }
    }
}

/// 内存中的配置状态。
struct Configuration {
    config: AppConfig,
    /// 配置文件无法读取时的错误码。此时**禁止写盘**，
    /// 否则会用默认值覆盖用户可能还能手工修复的文件。
    load_error: Option<&'static str>,
}

/// 应用共享状态：Tauri 命令与 IPC 线程都通过它读写配置与待确认请求。
struct AppState {
    config_path: PathBuf,
    configuration: Mutex<Configuration>,
    pending: Mutex<Option<PendingPush>>,
    app_handle: Mutex<Option<AppHandle>>,
    /// IPC 服务启动失败时的稳定错误码。
    ipc_error: Mutex<Option<&'static str>>,
}

/// 取互斥锁；锁中毒时继续使用内部数据。
///
/// 锁中毒意味着某个线程 panic 过，但配置与待确认请求都是普通数据，
/// 继续使用比让整个应用不可用更合理；安全判断不依赖这里的锁状态。
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl RepositoryView {
    fn from_config(repo: &gitguard_config::RepoConfig) -> Self {
        Self {
            repository_id: repo.repository_id.clone(),
            canonical_path: repo.canonical_path.clone(),
            push_confirmation_enabled: repo.push_confirmation_enabled,
            hook_status: repo.hook_status,
        }
    }
}

impl AppState {
    /// 从磁盘加载配置；读取失败时使用默认值并记录错误码，
    /// 之后所有写操作都会拒绝，避免覆盖用户文件。
    fn new(config_path: PathBuf) -> Self {
        let (config, load_error) = match gitguard_config::load_config_from(&config_path) {
            Ok(config) => (config, None),
            Err(error) => {
                eprintln!(
                    "gitguard: could not load the configuration at {}: {error}",
                    config_path.display()
                );
                (AppConfig::default(), Some(ERROR_CONFIGURATION_UNREADABLE))
            }
        };

        Self {
            config_path,
            configuration: Mutex::new(Configuration { config, load_error }),
            pending: Mutex::new(None),
            app_handle: Mutex::new(None),
            ipc_error: Mutex::new(None),
        }
    }

    /// 记录 IPC 服务启动失败；设置页会据此提示「Hook 无法连接桌面端」。
    fn set_ipc_error(&self, code: &'static str) {
        *lock(&self.ipc_error) = Some(code);
    }

    /// 把内存中的配置写回磁盘；配置曾读取失败时拒绝写入。
    fn persist(&self, configuration: &Configuration) -> Result<()> {
        if let Some(code) = configuration.load_error {
            return Err(GitGuardError::InvalidInput(format!(
                "configuration is not writable because it could not be loaded ({code})"
            )));
        }
        gitguard_config::save_config_to(&configuration.config, &self.config_path)
    }

    /// 组装设置页数据；同时刷新每个仓库的 Hook 健康状态。
    fn settings(&self) -> SettingsView {
        let mut configuration = lock(&self.configuration);

        let mut changed = false;
        for repo in &mut configuration.config.repositories {
            let status = gitguard_hook::check_hook_health(Path::new(&repo.canonical_path));
            if repo.hook_status != status {
                repo.hook_status = status;
                changed = true;
            }
        }
        if changed {
            if let Err(error) = self.persist(&configuration) {
                // 写盘失败不影响展示：内存里的状态仍然是最新的。
                eprintln!("gitguard: could not save refreshed hook status: {error}");
            }
        }

        let repositories = configuration
            .config
            .repositories
            .iter()
            .map(RepositoryView::from_config)
            .collect();

        SettingsView {
            global: GlobalSettingsView {
                require_confirmation_for_unregistered_repositories: configuration
                    .config
                    .global
                    .require_confirmation_for_unregistered_repositories,
                confirmation_timeout_seconds: configuration
                    .config
                    .global
                    .confirmation_timeout_seconds,
            },
            repositories,
            config_error: configuration.load_error,
            ipc_error: *lock(&self.ipc_error),
            config_path: self.config_path.to_string_lossy().into_owned(),
        }
    }

    /// 登记仓库并安装 Hook。
    ///
    /// 顺序是「先安装 Hook、后记录配置」：安装失败就不留下「已保护」的假象；
    /// 记录失败时 Hook 已就位，界面上的状态会在下次刷新时修正为真实值。
    ///
    /// 已经登记过的路径只刷新 Hook 脚本（升级后用来清掉旧脚本的绕过判断），
    /// 沿用原来的 `repository_id` 与保护开关：重新添加不是「关闭保护」的捷径，
    /// 开关只能通过 [`Self::set_protection`] 显式修改。
    fn add_repository(&self, path: &Path, cli: &Path, enabled: bool) -> Result<RepositoryView> {
        // 允许用户选择仓库内的子目录，统一解析成仓库根目录。
        let canonical = gitguard_git::repo_canonical_path(path)?;
        let key = canonical.to_str().ok_or_else(|| {
            GitGuardError::InvalidInput(format!(
                "repository path {} is not valid UTF-8",
                canonical.display()
            ))
        })?;

        let mut configuration = lock(&self.configuration);
        let (repository_id, enabled) = match gitguard_config::find_repo(&configuration.config, key)
        {
            Some(existing) => (
                existing.repository_id.clone(),
                existing.push_confirmation_enabled,
            ),
            None => (Uuid::new_v4().to_string(), enabled),
        };

        // Hook 里的仓库 id 必须与配置记录一致，CLI 才能判断记录是否丢失。
        gitguard_hook::install_hook(&canonical, cli, &repository_id)?;
        let status = gitguard_hook::check_hook_health(&canonical);

        let view = {
            let repo = gitguard_config::add_or_update_repo(
                &mut configuration.config,
                key.to_string(),
                repository_id,
                enabled,
            );
            repo.hook_status = status;
            RepositoryView::from_config(repo)
        };
        self.persist(&configuration)?;

        Ok(view)
    }

    /// 切换某个仓库的推送保护开关。
    fn set_protection(&self, repository_id: &str, enabled: bool) -> Result<()> {
        let mut configuration = lock(&self.configuration);
        let repo = configuration
            .config
            .repositories
            .iter_mut()
            .find(|repo| repo.repository_id == repository_id)
            .ok_or_else(|| {
                GitGuardError::InvalidInput(format!("unknown repository id {repository_id}"))
            })?;
        repo.push_confirmation_enabled = enabled;
        self.persist(&configuration)
    }

    /// 修改「未登记的仓库是否需要推送确认」。
    ///
    /// 只影响未登记仓库；已登记仓库的开关不会被批量改写。
    fn set_unregistered_protection(&self, enabled: bool) -> Result<()> {
        let mut configuration = lock(&self.configuration);
        configuration
            .config
            .global
            .require_confirmation_for_unregistered_repositories = enabled;
        self.persist(&configuration)
    }

    /// 修改确认超时时间；超出允许区间直接拒绝，不静默夹取。
    fn set_timeout(&self, seconds: u64) -> Result<()> {
        if !(MIN_TIMEOUT_SECONDS..=MAX_TIMEOUT_SECONDS).contains(&seconds) {
            return Err(GitGuardError::InvalidInput(format!(
                "confirmation timeout must be between {MIN_TIMEOUT_SECONDS} and {MAX_TIMEOUT_SECONDS} seconds"
            )));
        }

        let mut configuration = lock(&self.configuration);
        configuration.config.global.confirmation_timeout_seconds = seconds;
        self.persist(&configuration)
    }

    /// 当前配置下的确认超时时间（秒）。
    fn timeout_seconds(&self) -> u64 {
        lock(&self.configuration)
            .config
            .global
            .confirmation_timeout_seconds
            .clamp(MIN_TIMEOUT_SECONDS, MAX_TIMEOUT_SECONDS)
    }

    /// 取出与 `request_id` 匹配的待确认请求；ID 不匹配时保留现有请求。
    fn take_pending(&self, request_id: &str) -> Option<PendingPush> {
        let mut pending = lock(&self.pending);
        match pending.as_ref() {
            Some(current) if current.view.request_id == request_id => pending.take(),
            _ => None,
        }
    }

    /// 超时后清理待确认请求；只清理同一条，避免误删更新的请求。
    fn clear_pending(&self, request_id: &str) {
        let mut pending = lock(&self.pending);
        if pending
            .as_ref()
            .is_some_and(|current| current.view.request_id == request_id)
        {
            pending.take();
        }
    }

    /// 用户确认：重新校验 ref 后告诉 Hook「已确认」。
    ///
    /// 真正执行推送的是原始 `git push`，本进程不执行任何 Git 命令。
    /// 校验失败时先告诉 Hook「出错」，Hook 因此会拒绝本次 push；
    /// 校验通过才发出 `Confirmed`，Hook 以退出码 0 放行。
    fn confirm_push(&self, request_id: &str) -> std::result::Result<(), ConfirmationError> {
        let pending = self
            .take_pending(request_id)
            .ok_or(ConfirmationError::NoPendingRequest)?;

        // 已经到了桌面端自己的截止时间：CLI 多半已在它的超时边界放弃等待，
        // 此时再回「已确认」只会让界面谎报成功，因此按超时处理。
        if Instant::now() >= pending.deadline {
            let _ = pending.decision_tx.send(ConfirmationDecision::Timeout);
            return Err(ConfirmationError::NoPendingRequest);
        }

        let repo = Path::new(&pending.request.repository_path);
        if let Err(error) =
            gitguard_git::verify_push_preconditions(repo, &pending.request.ref_updates)
        {
            let _ = pending.decision_tx.send(ConfirmationDecision::Error);
            return Err(ConfirmationError::RefChanged(error));
        }

        // Hook 已超时离开时不能再回「已确认」，否则界面显示成功但用户什么都没推。
        if pending
            .decision_tx
            .send(ConfirmationDecision::Confirmed)
            .is_err()
        {
            return Err(ConfirmationError::NoPendingRequest);
        }

        Ok(())
    }

    /// 用户取消：唤醒 Hook 并让它拒绝 push。
    fn cancel_push(&self, request_id: &str) -> bool {
        match self.take_pending(request_id) {
            Some(pending) => {
                let _ = pending.decision_tx.send(ConfirmationDecision::Cancelled);
                true
            }
            None => false,
        }
    }

    /// 通知前端有新的待确认请求，并把窗口带到前台。
    ///
    /// 图形客户端（SourceTree、VS Code）里执行 push 时用户正盯着客户端，
    /// 确认窗口若在后台，很容易一直等到超时。窗口操作只是提醒手段，
    /// 失败不影响安全判断，因此统一忽略错误。app handle 尚未就绪时跳过（前端也会主动查询）。
    fn notify_frontend(&self, view: &PushConfirmationView) {
        let handle = lock(&self.app_handle).clone();
        let Some(handle) = handle else {
            return;
        };
        // 事件只是「刷新提示」，发送失败不影响安全判断。
        let _ = handle.emit(EVENT_PUSH_REQUEST_RECEIVED, view);

        if let Some(window) = handle.get_webview_window(MAIN_WINDOW_LABEL) {
            let _ = window.show();
            let _ = window.set_focus();
            // 窗口已被其它应用遮住时，再请求一次系统级的注意力提示。
            let _ = window.request_user_attention(Some(UserAttentionType::Critical));
        }
    }

    /// 把 Hook 请求整理成展示数据（含每个 ref 的提交列表）。
    ///
    /// 列不出提交就无法让用户判断「要推什么」，因此任何一步失败都返回错误，
    /// 由调用方直接拒绝，而不是让用户盲签。
    fn build_view(
        &self,
        request: &PushConfirmationRequest,
        timeout_seconds: u64,
    ) -> Result<PushConfirmationView> {
        let repo = Path::new(&request.repository_path);
        let mut refs = Vec::with_capacity(request.ref_updates.len());

        for update in &request.ref_updates {
            let kind = if update.is_deletion() {
                "delete"
            } else if update.is_creation() {
                "create"
            } else {
                "update"
            };
            // 删除远端 ref 时本地没有对象，不需要（也无法）列提交。
            let commits = if update.is_deletion() {
                Vec::new()
            } else {
                // 用请求里的 remote 名排除目标远端已有的提交，避免预览范围与实际不符。
                gitguard_git::list_commits(
                    repo,
                    &request.remote_name,
                    &update.remote_oid,
                    &update.local_oid,
                )?
            };
            refs.push(RefPreviewView {
                local_ref: update.local_ref.clone(),
                remote_ref: update.remote_ref.clone(),
                kind,
                commits,
            });
        }

        Ok(PushConfirmationView {
            request_id: request.request_id.clone(),
            repository_path: request.repository_path.clone(),
            remote_name: request.remote_name.clone(),
            remote_url: request.remote_url.clone(),
            refs,
            timeout_seconds,
            received_at_epoch_secs: now_epoch_secs(),
        })
    }
}

/// IPC 处理器实现：桌面端就是 Hook 的确认服务端。
impl IpcHandler for AppState {
    fn handle_push_request(&self, request: PushConfirmationRequest) -> PushConfirmationResponse {
        let request_id = request.request_id.clone();
        let error_response =
            |decision: ConfirmationDecision, message: &str| PushConfirmationResponse {
                request_id: request_id.clone(),
                decision,
                message: message.to_string(),
            };

        // 列不出提交就无法让用户判断「要推什么」，此时只能拒绝。
        let timeout_seconds = self.timeout_seconds();
        let view = match self.build_view(&request, timeout_seconds) {
            Ok(view) => view,
            Err(error) => {
                eprintln!("gitguard: could not preview the push request {request_id}: {error}");
                return error_response(ConfirmationDecision::Error, "commit list unavailable");
            }
        };

        let (decision_tx, decision_rx) = mpsc::channel();
        let deadline = Instant::now() + confirmation_wait_budget(timeout_seconds);
        {
            let mut pending = lock(&self.pending);
            if let Some(previous) = pending.replace(PendingPush {
                request,
                view: view.clone(),
                decision_tx,
                deadline,
            }) {
                // 同一时间只处理一个请求：新请求到来时，旧请求按取消处理。
                let _ = previous.decision_tx.send(ConfirmationDecision::Cancelled);
            }
        }
        self.notify_frontend(&view);

        match decision_rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(decision) => PushConfirmationResponse {
                request_id,
                decision,
                message: String::new(),
            },
            Err(RecvTimeoutError::Timeout) => {
                self.clear_pending(&request_id);
                error_response(ConfirmationDecision::Timeout, "confirmation timed out")
            }
            Err(RecvTimeoutError::Disconnected) => {
                self.clear_pending(&request_id);
                error_response(ConfirmationDecision::Error, "no decision was produced")
            }
        }
    }
}

/// 当前 Unix 时间戳（秒）；仅用于计算倒计时。
fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// 桌面端等待用户决定的时长：比配置值少一段余量，保证回执能在 CLI 放弃前送出。
fn confirmation_wait_budget(timeout_seconds: u64) -> Duration {
    Duration::from_secs(timeout_seconds)
        .saturating_sub(CONFIRMATION_DEADLINE_MARGIN)
        .max(MIN_CONFIRMATION_WAIT)
}

/// 找到要写进 Hook 的 `gitguard` 二进制路径。
///
/// 优先使用 `GITGUARD_CLI_PATH`；否则取当前可执行文件同目录下的 `gitguard`
/// （开发时 `cargo build` 会把它放在 `target/debug/` 里）。
/// 找不到时返回错误，绝不写入一个猜测出来的路径。
fn resolve_cli_binary() -> Result<PathBuf> {
    if let Some(value) = std::env::var_os(CLI_PATH_ENV) {
        let path = PathBuf::from(value);
        if path.is_absolute() && path.is_file() {
            return Ok(path);
        }
        return Err(GitGuardError::Internal(format!(
            "{CLI_PATH_ENV} must point to an existing absolute path"
        )));
    }

    let exe = std::env::current_exe()
        .map_err(|error| GitGuardError::Io(format!("could not locate the current exe: {error}")))?;
    let dir = exe
        .parent()
        .ok_or_else(|| GitGuardError::Internal("the current exe has no parent directory".into()))?;
    let candidate = dir.join(CLI_BINARY_NAME);
    if candidate.is_file() {
        Ok(candidate)
    } else {
        Err(GitGuardError::Internal(format!(
            "could not find the gitguard CLI next to {}; set {CLI_PATH_ENV}",
            exe.display()
        )))
    }
}

/// 前端通过 `invoke("get_pending_push_request")` 获取当前待确认请求。
#[tauri::command]
fn get_pending_push_request(state: State<'_, Arc<AppState>>) -> Option<PushConfirmationView> {
    lock(&state.pending)
        .as_ref()
        .map(|pending| pending.view.clone())
}

/// 用户点击「确认并推送」。校验放到阻塞线程池，避免卡住界面。
#[tauri::command]
async fn confirm_push(
    request_id: String,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), CommandError> {
    let app_state = Arc::clone(state.inner());

    let outcome = tauri::async_runtime::spawn_blocking(move || app_state.confirm_push(&request_id))
        .await
        .map_err(|error| {
            eprintln!("gitguard: the confirmation task failed: {error}");
            command_error(ERROR_INTERNAL)
        })?;

    outcome.map_err(|error| {
        eprintln!("gitguard: could not confirm the push: {error}");
        match error {
            ConfirmationError::NoPendingRequest => command_error(ERROR_NO_PENDING_REQUEST),
            ConfirmationError::RefChanged(_) => command_error(ERROR_REQUEST_INVALID),
        }
    })
}

/// 用户点击「取消」。
#[tauri::command]
fn cancel_push(
    request_id: String,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), CommandError> {
    if state.cancel_push(&request_id) {
        Ok(())
    } else {
        Err(command_error(ERROR_NO_PENDING_REQUEST))
    }
}

/// 设置页数据（全局设置 + 仓库列表 + 配置/IPC 错误）。
#[tauri::command]
fn get_settings(state: State<'_, Arc<AppState>>) -> SettingsView {
    state.settings()
}

/// 按界面语言重建原生菜单。
///
/// 菜单只是展示层：未知 locale 回落到默认语言，重建失败也只写 stderr，
/// 不影响界面与任何确认或放行判断。
#[tauri::command]
fn set_app_menu(locale: String, app: tauri::AppHandle) -> std::result::Result<(), CommandError> {
    let menu =
        menu::build_menu(&app, menu::MenuLanguage::from_locale(&locale)).map_err(|error| {
            eprintln!("gitguard: could not build the application menu: {error}");
            command_error(ERROR_INTERNAL)
        })?;

    app.set_menu(menu).map_err(|error| {
        eprintln!("gitguard: could not apply the application menu: {error}");
        command_error(ERROR_INTERNAL)
    })?;

    Ok(())
}

/// 校验并归一化用户填写的仓库路径。
///
/// 只接受绝对路径：相对路径会按桌面端进程的当前目录解析（从 Finder 启动时通常是 `/`），
/// 结果不可预期。目录是否存在也要先确认，否则 `git rev-parse` 的错误会更难理解。
fn resolve_repository_path(path: &str) -> std::result::Result<PathBuf, CommandError> {
    let target = PathBuf::from(path);
    if !target.is_absolute() {
        return Err(command_error(ERROR_PATH_NOT_ABSOLUTE));
    }
    if !target.is_dir() {
        return Err(command_error(ERROR_NOT_A_REPOSITORY));
    }
    Ok(target)
}

/// 添加仓库并安装 Hook；`enabled` 决定该仓库的推送保护开关。
#[tauri::command]
fn add_repository(
    path: String,
    enabled: bool,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<RepositoryView, CommandError> {
    let target = resolve_repository_path(&path)?;

    let cli = resolve_cli_binary().map_err(|error| {
        eprintln!("gitguard: {error}");
        command_error(ERROR_CLI_NOT_FOUND)
    })?;

    state
        .add_repository(&target, &cli, enabled)
        .map_err(|error| {
            eprintln!("gitguard: could not add repository {path}: {error}");
            match error {
                GitGuardError::Internal(_) => command_error(ERROR_NOT_A_REPOSITORY),
                _ => command_error(ERROR_HOOK_INSTALL_FAILED),
            }
        })
}

/// 切换仓库的推送保护开关。
#[tauri::command]
fn toggle_repo_protection(
    repository_id: String,
    enabled: bool,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), CommandError> {
    state
        .set_protection(&repository_id, enabled)
        .map_err(|error| {
            eprintln!("gitguard: could not toggle protection: {error}");
            if matches!(error, GitGuardError::InvalidInput(_)) {
                command_error(ERROR_REPOSITORY_NOT_FOUND)
            } else {
                command_error(ERROR_CONFIGURATION_WRITE_FAILED)
            }
        })
}

/// 修改未登记仓库的推送确认开关。
#[tauri::command]
fn set_unregistered_protection(
    enabled: bool,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), CommandError> {
    state.set_unregistered_protection(enabled).map_err(|error| {
        eprintln!("gitguard: could not change the unregistered repository setting: {error}");
        command_error(ERROR_CONFIGURATION_WRITE_FAILED)
    })
}

/// 设置确认超时时间（秒）。
#[tauri::command]
fn set_confirmation_timeout(
    seconds: u64,
    state: State<'_, Arc<AppState>>,
) -> std::result::Result<(), CommandError> {
    state.set_timeout(seconds).map_err(|error| {
        eprintln!("gitguard: could not set the confirmation timeout: {error}");
        if matches!(error, GitGuardError::InvalidInput(_)) {
            command_error(ERROR_INVALID_TIMEOUT)
        } else {
            command_error(ERROR_CONFIGURATION_WRITE_FAILED)
        }
    })
}

/// 启动 IPC 服务；失败时记录稳定错误码，界面会提示「桌面端未被 Hook 使用」。
fn start_ipc_server(state: &Arc<AppState>) -> Option<IpcServer> {
    let socket_path = match gitguard_ipc::default_socket_path() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("gitguard: could not resolve the IPC socket path: {error}");
            return None;
        }
    };

    match IpcServer::start(socket_path, Arc::clone(state)) {
        Ok(server) => Some(server),
        Err(error) => {
            eprintln!("gitguard: could not start the IPC server: {error}");
            None
        }
    }
}

/// 构建并启动 Tauri 应用；注册的命令名必须与前端 `invoke` 的名称一致。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 启动期不可恢复错误：拿不到配置目录时 panic，而不是猜测路径，
    // 否则桌面端与 Hook 会读写不同的配置文件。
    let config_path = gitguard_config::config_file_path()
        .expect("the platform configuration directory must be available at startup");

    let state = Arc::new(AppState::new(config_path));

    // 服务必须活到进程退出；`_ipc_server` 在 `run()` 返回后才会被丢弃。
    let ipc_server = start_ipc_server(&state);
    if ipc_server.is_none() {
        state.set_ipc_error(ERROR_IPC_UNAVAILABLE);
    }
    let _ipc_server = ipc_server;

    let setup_state = Arc::clone(&state);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // 菜单只通知前端打开设置页；菜单项本身不承载业务逻辑。
        .on_menu_event(|app, event| {
            if event.id() == menu::SETTINGS_MENU_ID {
                let _ = app.emit(menu::EVENT_OPEN_SETTINGS, ());
            }
        })
        .manage(state)
        .setup(move |app| {
            // 保存 AppHandle，IPC 线程才能向前端推送事件。
            *lock(&setup_state.app_handle) = Some(app.handle().clone());
            // 先用默认语言建好菜单，前端挂载后会按实际界面语言重建一次。
            let menu = menu::build_menu(app.handle(), menu::MenuLanguage::ZhCn)?;
            app.set_menu(menu)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_pending_push_request,
            confirm_push,
            cancel_push,
            get_settings,
            set_app_menu,
            add_repository,
            toggle_repo_protection,
            set_unregistered_protection,
            set_confirmation_timeout,
        ])
        .run(tauri::generate_context!())
        // 启动期不可恢复错误：此时还没有窗口与日志通道，只能 panic 并打印原因。
        .expect("error while running GitGuard");
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use tempfile::TempDir;

    use super::*;

    /// 临时 Git 仓库，用于验证「添加仓库 → 安装 Hook → 记录配置」。
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
    }

    fn git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .expect("execute git");
        assert!(
            output.status.success(),
            "git {args:?} failed:\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// 建立一个使用临时配置文件的状态，绝不读写用户真实配置。
    fn temp_state(temp: &TempDir) -> AppState {
        AppState::new(temp.path().join("config.json"))
    }

    /// 往状态里放一条待确认请求，返回等待决定的一端；`deadline` 用于模拟超时。
    fn insert_pending(state: &AppState, deadline: Instant) -> mpsc::Receiver<ConfirmationDecision> {
        let (decision_tx, decision_rx) = mpsc::channel();
        let request = PushConfirmationRequest {
            request_id: "request-1".into(),
            repository_path: "/tmp/example-repository".into(),
            remote_name: "origin".into(),
            remote_url: "git@example.com:team/project.git".into(),
            ref_updates: Vec::new(),
            timestamp_epoch_secs: 0,
        };
        let view = PushConfirmationView {
            request_id: request.request_id.clone(),
            repository_path: request.repository_path.clone(),
            remote_name: request.remote_name.clone(),
            remote_url: request.remote_url.clone(),
            refs: Vec::new(),
            timeout_seconds: MIN_TIMEOUT_SECONDS,
            received_at_epoch_secs: 0,
        };
        *lock(&state.pending) = Some(PendingPush {
            request,
            view,
            decision_tx,
            deadline,
        });
        decision_rx
    }

    #[test]
    fn state_starts_with_defaults_when_no_config_file_exists() {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let state = temp_state(&temp);
        let settings = state.settings();

        // 未登记的仓库默认不拦截。
        assert!(
            !settings
                .global
                .require_confirmation_for_unregistered_repositories
        );
        assert_eq!(settings.global.confirmation_timeout_seconds, 180);
        assert!(settings.repositories.is_empty());
        assert!(settings.config_error.is_none());
        assert!(settings.ipc_error.is_none());
        assert!(
            settings.config_path.ends_with("config.json"),
            "设置页需要展示实际使用的配置文件路径"
        );
    }

    #[test]
    fn settings_view_exposes_the_config_path_in_camel_case() {
        // 前端按 camelCase 校验 `configPath`；字段名不一致会让设置页整体不可用。
        let temp = tempfile::tempdir().expect("create temporary directory");
        let state = temp_state(&temp);
        let json = serde_json::to_value(state.settings()).expect("settings should serialize");

        assert_eq!(
            json["configPath"].as_str(),
            Some(state.config_path.to_string_lossy().as_ref())
        );
        assert!(
            json.get("config_path").is_none(),
            "前端不认 snake_case 字段名"
        );
    }

    #[test]
    fn add_repository_installs_a_hook_and_persists_the_entry() {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let state = temp_state(&temp);
        let repo = TestRepo::new();
        let cli = temp.path().join("gitguard");
        fs::write(&cli, "#!/bin/sh\n").expect("create fake cli");

        let view = state
            .add_repository(&repo.root, &cli, true)
            .expect("repository should be added");

        assert_eq!(view.hook_status, HookStatus::Healthy);
        assert!(view.push_confirmation_enabled);
        assert_eq!(
            Path::new(&view.canonical_path),
            repo.root.canonicalize().expect("canonicalize repository")
        );

        let persisted = gitguard_config::load_config_from(&temp.path().join("config.json"))
            .expect("saved config should load");
        assert_eq!(persisted.repositories.len(), 1);
        assert_eq!(
            persisted.repositories[0].canonical_path,
            view.canonical_path
        );
    }

    #[test]
    fn adding_a_registered_repository_keeps_its_switch_and_id() {
        // 回归：升级后按 README 重新添加已登记仓库，只应刷新 Hook 脚本；
        // 表单默认不勾选，但重新添加不得把已经开着的保护静默关掉。
        let temp = tempfile::tempdir().expect("create temporary directory");
        let state = temp_state(&temp);
        let repo = TestRepo::new();
        let cli = temp.path().join("gitguard");
        fs::write(&cli, "#!/bin/sh\n").expect("create fake cli");

        let first = state
            .add_repository(&repo.root, &cli, true)
            .expect("repository should be added");
        let second = state
            .add_repository(&repo.root, &cli, false)
            .expect("repository should be re-added");

        assert!(second.push_confirmation_enabled, "重新添加不得关闭保护");
        assert_eq!(
            second.repository_id, first.repository_id,
            "同一仓库的记录 id 必须保持稳定"
        );

        // Hook 脚本里的 id 必须与记录一致，否则 CLI 会把它当成记录丢失。
        let script = fs::read_to_string(
            gitguard_hook::hook_paths(&repo.root)
                .expect("resolve hook paths")
                .hook,
        )
        .expect("read installed hook");
        assert!(
            script.contains(&first.repository_id),
            "Hook 必须携带登记记录的 id"
        );
    }

    #[test]
    fn add_repository_reports_not_a_repository_for_plain_directories() {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let state = temp_state(&temp);
        let cli = temp.path().join("gitguard");
        fs::write(&cli, "#!/bin/sh\n").expect("create fake cli");

        let error = state
            .add_repository(temp.path(), &cli, true)
            .expect_err("a plain directory is not a repository");
        assert!(matches!(error, GitGuardError::Internal(_)));
    }

    #[test]
    fn protection_and_timeout_changes_are_persisted() {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let state = temp_state(&temp);
        let repo = TestRepo::new();
        let cli = temp.path().join("gitguard");
        fs::write(&cli, "#!/bin/sh\n").expect("create fake cli");
        let view = state
            .add_repository(&repo.root, &cli, true)
            .expect("repository should be added");

        state
            .set_protection(&view.repository_id, false)
            .expect("disable protection");
        state.set_timeout(42).expect("set timeout");

        let settings = state.settings();
        assert!(!settings.repositories[0].push_confirmation_enabled);
        assert_eq!(settings.global.confirmation_timeout_seconds, 42);
    }

    #[test]
    fn timeout_must_stay_within_the_allowed_range() {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let state = temp_state(&temp);

        for seconds in [0, MAX_TIMEOUT_SECONDS + 1] {
            assert!(matches!(
                state.set_timeout(seconds),
                Err(GitGuardError::InvalidInput(_))
            ));
        }
        state
            .set_timeout(MIN_TIMEOUT_SECONDS)
            .expect("minimum is allowed");
    }

    #[test]
    fn unregistered_setting_and_add_time_switch_are_persisted() {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let state = temp_state(&temp);
        let repo = TestRepo::new();
        let cli = temp.path().join("gitguard");
        fs::write(&cli, "#!/bin/sh\n").expect("create fake cli");

        state
            .set_unregistered_protection(true)
            .expect("change the unregistered setting");
        // 添加时就关闭保护：全局开关不参与，只看该仓库自己的开关。
        let view = state
            .add_repository(&repo.root, &cli, false)
            .expect("repository should be added");

        assert!(!view.push_confirmation_enabled);
        let settings = state.settings();
        assert!(
            settings
                .global
                .require_confirmation_for_unregistered_repositories
        );
        assert!(!settings.repositories[0].push_confirmation_enabled);

        // 落盘后重新加载仍然一致，避免只改了内存。
        let persisted = gitguard_config::load_config_from(&temp.path().join("config.json"))
            .expect("saved config should load");
        assert!(
            persisted
                .global
                .require_confirmation_for_unregistered_repositories
        );
        assert!(!persisted.repositories[0].push_confirmation_enabled);
    }

    #[test]
    fn relative_repository_paths_are_rejected() {
        let error = resolve_repository_path("relative/repository")
            .expect_err("a relative path must be rejected");
        assert_eq!(error.code, ERROR_PATH_NOT_ABSOLUTE);

        let missing = tempfile::tempdir()
            .expect("create temporary directory")
            .path()
            .join("not-a-repository");
        let error = resolve_repository_path(&missing.to_string_lossy())
            .expect_err("a missing directory must be rejected");
        assert_eq!(error.code, ERROR_NOT_A_REPOSITORY);
    }

    #[test]
    fn unknown_repository_ids_are_rejected() {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let state = temp_state(&temp);

        assert!(matches!(
            state.set_protection("missing-id", true),
            Err(GitGuardError::InvalidInput(_))
        ));
    }

    #[test]
    fn confirmation_view_serializes_commit_fields_for_the_frontend_contract() {
        // 前端按 camelCase 校验待确认请求（`shortSha`）。字段名一旦不一致，
        // 前端会把请求当成「没有待确认请求」而静默忽略，Hook 只能等到超时——
        // 用户看到的现象就是「git push 没有任何反应」。
        let view = PushConfirmationView {
            request_id: "request-1".into(),
            repository_path: "/tmp/repo".into(),
            remote_name: "origin".into(),
            remote_url: "git@example.com:team/project.git".into(),
            refs: vec![RefPreviewView {
                local_ref: "refs/heads/main".into(),
                remote_ref: "refs/heads/main".into(),
                kind: "update",
                commits: vec![CommitInfo {
                    sha: "a".repeat(40),
                    short_sha: "aaaaaaa".into(),
                    author: "Ada".into(),
                    timestamp: "2026-10-24T09:30:00+08:00".into(),
                    subject: "feat: first".into(),
                }],
            }],
            timeout_seconds: 180,
            received_at_epoch_secs: 1_760_000_000,
        };

        let json = serde_json::to_value(&view).expect("the view should serialize to JSON");
        let commit = &json["refs"][0]["commits"][0];
        assert_eq!(commit["shortSha"], "aaaaaaa");
        assert!(
            commit.get("short_sha").is_none(),
            "前端不认 snake_case 字段名"
        );
        assert_eq!(json["requestId"], "request-1");
        assert_eq!(json["refs"][0]["localRef"], "refs/heads/main");
    }

    #[test]
    fn confirming_without_a_pending_request_fails() {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let state = temp_state(&temp);

        assert!(matches!(
            state.confirm_push("unknown-request"),
            Err(ConfirmationError::NoPendingRequest)
        ));
        assert!(!state.cancel_push("unknown-request"));
    }

    #[test]
    fn confirming_after_the_deadline_is_refused() {
        // 回归：CLI 已在自己的超时边界放弃等待时，桌面端不能再回「已确认」，
        // 否则界面提示「原 git push 将继续」，而实际推送早已被拒绝。
        let temp = tempfile::tempdir().expect("create temporary directory");
        let state = temp_state(&temp);
        let decisions = insert_pending(&state, Instant::now() - Duration::from_millis(1));

        assert!(matches!(
            state.confirm_push("request-1"),
            Err(ConfirmationError::NoPendingRequest)
        ));
        // 仍在等待的 Hook 必须收到明确的超时，而不是被静默丢弃。
        assert_eq!(decisions.try_recv(), Ok(ConfirmationDecision::Timeout));
        assert!(lock(&state.pending).is_none(), "过期请求必须被清理");
    }

    #[test]
    fn the_desktop_stops_waiting_before_the_cli_gives_up() {
        // 桌面端的等待时长必须小于 CLI 的超时，回执才来得及送回仍活着的 Hook。
        for seconds in [MIN_TIMEOUT_SECONDS, 60, MAX_TIMEOUT_SECONDS] {
            let budget = confirmation_wait_budget(seconds);
            assert!(
                budget < Duration::from_secs(seconds),
                "{seconds}s 的等待预算必须小于配置值"
            );
            assert!(budget >= MIN_CONFIRMATION_WAIT);
        }
    }

    #[test]
    fn a_corrupt_config_file_is_reported_and_never_overwritten() {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let config_path = temp.path().join("config.json");
        fs::write(&config_path, b"{ not json }").expect("write corrupt config");

        let state = AppState::new(config_path.clone());
        let settings = state.settings();
        assert_eq!(settings.config_error, Some(ERROR_CONFIGURATION_UNREADABLE));

        // 读写都必须拒绝，否则会把用户可能手工可修复的文件覆盖掉。
        assert!(state.set_timeout(30).is_err());
        assert_eq!(
            fs::read(&config_path).expect("config file should be untouched"),
            b"{ not json }"
        );
    }
}
