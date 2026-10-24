//! GitGuard 本地 IPC：Hook（CLI）与桌面端通过 Unix Domain Socket 交换推送确认请求。
//!
//! 协议非常简单：每条消息是一个 JSON 对象，以换行符 `\n` 结尾；一条连接只处理
//! 一次「请求 → 响应」，处理完就关闭。这样既不需要帧长度前缀，也方便用
//! `BufRead::read_line` 逐行读取。
//!
//! 依赖方向是 `gitguard-cli / gitguard-desktop → gitguard-ipc → gitguard-core`。
//! 本 crate 只负责「传输」，不做任何策略判断：策略与最终放行决定必须留在
//! `gitguard-policy` / `gitguard-cli` 里。任何通信失败都返回
//! [`GitGuardError::IpcError`]，调用方必须把它当作拒绝处理（fail-closed）。
//!
//! 平台支持：仅 Unix（macOS、Linux）。Windows 需要另行实现命名管道。

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use gitguard_core::{GitGuardError, PushConfirmationRequest, PushConfirmationResponse, Result};

/// 应用数据目录名，与 Tauri 的 bundle identifier 保持一致，便于桌面端与 Hook 找到同一个位置。
const APP_DIR_NAME: &str = "dev.gitguard.desktop";

/// Socket 文件名。
const SOCKET_FILE_NAME: &str = "gitguard.sock";

/// 单条消息允许的最大字节数（64 KiB）。
///
/// 确认请求只包含 ref 列表等短文本，正常远小于这个上限。设置上限是为了防止
/// 对端（或本机上的恶意进程）发送无限长的数据把内存撑爆；超过上限的消息会
/// 解析失败，连接被直接关闭，属于 fail-closed。
const MAX_MESSAGE_BYTES: u64 = 64 * 1024;

/// 服务端等待一个连接发来请求的最长时间（5 分钟）。
///
/// 用户可能在确认弹窗上停留很久，但无限等待会永久占用线程，因此必须有上限。
const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(300);

/// 写响应的超时时间；响应很小，30 秒足够，写不出去说明对端已经异常。
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

/// 监听线程在 `accept` 返回 `WouldBlock` 后的轮询间隔。
///
/// 监听 socket 被设为非阻塞，才能定期检查关闭标志；50 毫秒的延迟对人工确认
/// 场景完全无感，也不会空转占满 CPU。
const ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// 监听出现其他错误后的退避时间，避免错误持续发生时忙等。
const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(100);

/// 计算默认的 IPC socket 路径。
///
/// - macOS：`~/Library/Application Support/dev.gitguard.desktop/gitguard.sock`
/// - Linux：`~/.config/dev.gitguard.desktop/gitguard.sock`
///   （可用 `XDG_CONFIG_HOME` 覆盖；当前实现统一放在用户配置目录，便于 MVP 调试）
///
/// 如果系统无法给出配置目录（例如环境变量被清空且没有 home 目录），返回
/// [`GitGuardError::Internal`]，而不是猜一个路径——猜错会导致 Hook 与桌面端
/// 连到不同位置，Hook 因此拿不到确认，只能拒绝推送（安全但不可用）。
pub fn default_socket_path() -> Result<PathBuf> {
    let config_dir = dirs::config_dir().ok_or_else(|| {
        GitGuardError::Internal(
            "could not determine the platform config directory for the IPC socket".to_string(),
        )
    })?;
    Ok(config_dir.join(APP_DIR_NAME).join(SOCKET_FILE_NAME))
}

/// IPC 请求处理器。
///
/// 桌面端实现这个 trait，把 Hook 发来的请求变成用户在界面上的确认操作，并
/// 返回 [`PushConfirmationResponse`]。注意：这里返回的响应只是「传输结果」，
/// Hook 侧仍只认 [`gitguard_core::ConfirmationDecision::Confirmed`]，
/// 其余取值一律按拒绝处理。
pub trait IpcHandler: Send + Sync + 'static {
    /// 处理一次推送确认请求。
    ///
    /// 实现方不要 panic：panic 只会让处理该连接的线程退出，Hook 收不到响应，
    /// 最终仍会拒绝推送（fail-closed），但用户会看到「桌面端无响应」。
    fn handle_push_request(&self, request: PushConfirmationRequest) -> PushConfirmationResponse;
}

/// Unix Socket IPC 服务端（桌面端使用）。
///
/// 在后台线程监听连接，每个连接再交给独立线程处理，因此一个慢请求不会阻塞
/// 后续的连接。调用 [`IpcServer::stop`] 或直接 `drop` 都会关闭监听、等待监听
/// 线程退出并删除 socket 文件。
#[derive(Debug)]
pub struct IpcServer {
    /// 关闭标志；监听线程每轮循环都会检查它。
    shutdown: Arc<AtomicBool>,
    /// 监听线程句柄；`stop` 取出并 `join`，确保退出后不再有线程使用 socket。
    listener_thread: Option<std::thread::JoinHandle<()>>,
    /// 本服务绑定的 socket 路径，停止时用于清理。
    socket_path: PathBuf,
}

impl IpcServer {
    /// 在 `socket_path` 上启动服务，并用 `handler` 处理收到的请求。
    ///
    /// 启动时会：
    /// 1. 创建父目录；
    /// 2. 删除上次崩溃可能残留的 socket 文件（否则 `bind` 会报地址已被占用）；
    /// 3. 绑定并切换为非阻塞监听；
    /// 4. 把 socket 权限收紧为 `0600`，只允许当前用户连接。
    ///
    /// 任一步失败都会返回错误，不会带着半初始化状态继续运行。
    pub fn start<H: IpcHandler>(socket_path: PathBuf, handler: Arc<H>) -> Result<Self> {
        // 先保证目录存在；`parent()` 为空（例如只给了文件名）时跳过，交给 `bind` 报错。
        if let Some(parent) = socket_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).map_err(|error| {
                GitGuardError::Io(format!(
                    "failed to create the IPC socket directory: {error}"
                ))
            })?;
        }

        // 上次进程异常退出会留下 socket 文件。它已经不可用，必须先删掉才能重新绑定。
        match std::fs::remove_file(&socket_path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(GitGuardError::Io(format!(
                    "failed to remove the stale IPC socket: {error}"
                )));
            }
        }

        let listener = UnixListener::bind(&socket_path).map_err(|error| {
            GitGuardError::Io(format!("failed to bind the IPC socket: {error}"))
        })?;

        // 从这里开始 socket 文件已经存在；任何失败都要顺手清理，避免留下死 socket。
        if let Err(error) = configure_listener(&listener, &socket_path) {
            let _ = std::fs::remove_file(&socket_path);
            return Err(error);
        }

        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = Arc::clone(&shutdown);
        let path_clone = socket_path.clone();

        let listener_thread = std::thread::spawn(move || {
            Self::accept_loop(listener, handler, shutdown_clone);
            // 监听线程退出时清理 socket 文件，避免下次启动时误判为「已有实例在运行」。
            let _ = std::fs::remove_file(&path_clone);
        });

        Ok(Self {
            shutdown,
            listener_thread: Some(listener_thread),
            socket_path,
        })
    }

    /// 监听循环：直到 `shutdown` 被置位为止，不断接受新连接。
    fn accept_loop<H: IpcHandler>(
        listener: UnixListener,
        handler: Arc<H>,
        shutdown: Arc<AtomicBool>,
    ) {
        while !shutdown.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _address)) => {
                    let handler = Arc::clone(&handler);
                    // 每个连接一个线程：确认弹窗可能很慢，绝不能阻塞其他连接。
                    std::thread::spawn(move || {
                        Self::handle_connection(stream, &*handler);
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    // 非阻塞监听在没有新连接时会立刻返回 WouldBlock，稍等再试。
                    std::thread::sleep(ACCEPT_POLL_INTERVAL);
                }
                Err(_) => {
                    // 其他错误不终止服务：可能是瞬时资源不足，退避后继续。
                    // 这里不打印日志，避免把 socket 路径等信息写进 Hook 的 stderr。
                    std::thread::sleep(ACCEPT_ERROR_BACKOFF);
                }
            }
        }
    }

    /// 处理单条连接：读取一行 JSON 请求，交给 handler，再把响应写回一行 JSON。
    ///
    /// 任何一步失败都只是关闭这条连接，不会影响监听线程与其他连接。
    fn handle_connection<H: IpcHandler>(stream: UnixStream, handler: &H) {
        // 监听 socket 是非阻塞的，但 `accept` 返回的连接在部分平台会继承该标志，
        // 所以这里显式切回阻塞模式，并设置读写超时。
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(REQUEST_READ_TIMEOUT));
        let _ = stream.set_write_timeout(Some(WRITE_TIMEOUT));

        let mut request_line = String::new();
        // 限制单行长度，防止对端无限发送数据。
        let mut reader = BufReader::new(&stream).take(MAX_MESSAGE_BYTES);
        if reader.read_line(&mut request_line).is_err() || request_line.trim().is_empty() {
            return;
        }

        // 非法 JSON 直接断开，不回显解析错误，避免把内部细节暴露给调用方。
        let Ok(request) = serde_json::from_str::<PushConfirmationRequest>(&request_line) else {
            return;
        };

        let response = handler.handle_push_request(request);
        let Ok(json) = serde_json::to_string(&response) else {
            return;
        };

        // `Write` 对 `&UnixStream` 有实现，因此读取用的不可变借用不会妨碍写回。
        let mut writer = &stream;
        let _ = writeln!(writer, "{json}");
        let _ = writer.flush();
    }

    /// 停止服务：置位关闭标志 → 等待监听线程退出 → 删除 socket 文件。
    ///
    /// 可以重复调用；`Drop` 中也会调用一次，因此不需要手动 `stop` 也不会泄漏资源。
    /// 正在处理中的连接线程不会被强制中断，它们最多在读写超时后自行结束。
    pub fn stop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(thread) = self.listener_thread.take() {
            let _ = thread.join();
        }
        // 清理是尽力而为：文件可能已被监听线程或外部删除。
        let _ = std::fs::remove_file(&self.socket_path);
    }

    /// 返回本服务实际绑定的 socket 路径。
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 把监听 socket 切到非阻塞模式，并把 socket 文件权限收紧为仅当前用户可读写执行。
///
/// `0600` 表示只有属主可以连接：Unix Domain Socket 的连接权限取决于文件写权限，
/// 这样同机其他用户无法向桌面端发送伪造的确认请求。
fn configure_listener(listener: &UnixListener, socket_path: &Path) -> Result<()> {
    listener.set_nonblocking(true).map_err(|error| {
        GitGuardError::Io(format!(
            "failed to switch the IPC listener to non-blocking mode: {error}"
        ))
    })?;

    std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o600)).map_err(
        |error| {
            GitGuardError::Io(format!(
                "failed to restrict the IPC socket permissions: {error}"
            ))
        },
    )?;

    Ok(())
}

/// 向桌面端发送一次推送确认请求，并等待响应（CLI / Hook 使用）。
///
/// `timeout` 只限制「等待响应」的时间；连接与写入各自还有独立的超时。
/// 任何失败——桌面端没运行、超时、响应不是合法 JSON——都返回
/// [`GitGuardError::IpcError`]，调用方必须按拒绝处理。
pub fn send_confirmation_request(
    socket_path: &Path,
    request: &PushConfirmationRequest,
    timeout: Duration,
) -> Result<PushConfirmationResponse> {
    let stream = UnixStream::connect(socket_path).map_err(|error| {
        GitGuardError::IpcError(format!(
            "could not connect to the GitGuard desktop app: {error}"
        ))
    })?;

    stream.set_read_timeout(Some(timeout)).map_err(|error| {
        GitGuardError::IpcError(format!("failed to set the IPC read timeout: {error}"))
    })?;
    stream
        .set_write_timeout(Some(WRITE_TIMEOUT))
        .map_err(|error| {
            GitGuardError::IpcError(format!("failed to set the IPC write timeout: {error}"))
        })?;

    let json = serde_json::to_string(request).map_err(|error| {
        GitGuardError::Internal(format!(
            "failed to serialize the push confirmation request: {error}"
        ))
    })?;

    let mut writer = &stream;
    writeln!(writer, "{json}").map_err(|error| {
        GitGuardError::IpcError(format!(
            "failed to send the push confirmation request: {error}"
        ))
    })?;
    writer.flush().map_err(|error| {
        GitGuardError::IpcError(format!(
            "failed to flush the push confirmation request: {error}"
        ))
    })?;

    let mut response_line = String::new();
    let mut reader = BufReader::new(&stream).take(MAX_MESSAGE_BYTES);
    reader.read_line(&mut response_line).map_err(|error| {
        GitGuardError::IpcError(format!(
            "failed to read the push confirmation response: {error}"
        ))
    })?;

    // 对端在没回包的情况下关闭连接时，`read_line` 会成功但读到空串；
    // 这绝不能被当成「用户已确认」。
    if response_line.trim().is_empty() {
        return Err(GitGuardError::IpcError(
            "the desktop app closed the connection without a response".to_string(),
        ));
    }

    serde_json::from_str(&response_line).map_err(|error| {
        GitGuardError::IpcError(format!(
            "the desktop app returned an invalid response: {error}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitguard_core::{ConfirmationDecision, RefUpdate};
    use std::sync::atomic::AtomicUsize;

    /// 测试用处理器：记录被调用的次数，并按需模拟「用户思考很久」。
    struct ConfirmingHandler {
        calls: Arc<AtomicUsize>,
        delay: Duration,
    }

    impl ConfirmingHandler {
        fn new(delay: Duration) -> Self {
            Self {
                calls: Arc::new(AtomicUsize::new(0)),
                delay,
            }
        }
    }

    impl IpcHandler for ConfirmingHandler {
        fn handle_push_request(
            &self,
            request: PushConfirmationRequest,
        ) -> PushConfirmationResponse {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if !self.delay.is_zero() {
                std::thread::sleep(self.delay);
            }
            PushConfirmationResponse {
                request_id: request.request_id,
                decision: ConfirmationDecision::Confirmed,
                message: "approved in test".to_string(),
            }
        }
    }

    fn sample_request() -> PushConfirmationRequest {
        PushConfirmationRequest {
            request_id: "6f1c5d2e-4b8a-4c62-9dc0-1d2f3a4b5c6d".to_string(),
            repository_path: "/tmp/example-repository".to_string(),
            remote_name: "origin".to_string(),
            remote_url: "git@example.com:team/project.git".to_string(),
            ref_updates: vec![RefUpdate {
                local_ref: "refs/heads/main".to_string(),
                local_oid: "a".repeat(40),
                remote_ref: "refs/heads/main".to_string(),
                remote_oid: "b".repeat(40),
            }],
            timestamp_epoch_secs: 1_760_000_000,
        }
    }

    fn temp_socket_path(temp_dir: &tempfile::TempDir) -> PathBuf {
        temp_dir.path().join("gitguard.sock")
    }

    #[test]
    fn default_socket_path_points_into_the_app_directory() {
        let path = default_socket_path().expect("the config directory should be available");

        assert!(path.is_absolute());
        assert!(path.ends_with(Path::new(APP_DIR_NAME).join(SOCKET_FILE_NAME)));
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some(SOCKET_FILE_NAME)
        );
    }

    #[test]
    fn server_creates_a_private_socket_and_removes_it_on_stop() {
        let temp_dir = tempfile::tempdir().expect("a temporary directory should be created");
        let socket_path = temp_socket_path(&temp_dir);
        let mut server = IpcServer::start(
            socket_path.clone(),
            Arc::new(ConfirmingHandler::new(Duration::ZERO)),
        )
        .expect("the IPC server should start");

        assert!(socket_path.exists());
        assert_eq!(server.socket_path(), socket_path.as_path());
        // 只有当前用户可以连接，同机其他用户不能伪造确认请求。
        let mode = std::fs::metadata(&socket_path)
            .expect("the socket file should have metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);

        server.stop();
        assert!(!socket_path.exists());
        // 重复 stop（Drop 还会再调用一次）必须是安全的。
        server.stop();
    }

    #[test]
    fn client_receives_the_handler_response() {
        let temp_dir = tempfile::tempdir().expect("a temporary directory should be created");
        let socket_path = temp_socket_path(&temp_dir);
        let handler = Arc::new(ConfirmingHandler::new(Duration::ZERO));
        let calls = Arc::clone(&handler.calls);
        let server =
            IpcServer::start(socket_path.clone(), handler).expect("the IPC server should start");

        let request = sample_request();
        let response = send_confirmation_request(&socket_path, &request, Duration::from_secs(5))
            .expect("the round trip should succeed");

        assert_eq!(response.request_id, request.request_id);
        assert_eq!(response.decision, ConfirmationDecision::Confirmed);
        assert_eq!(calls.load(Ordering::Relaxed), 1);

        drop(server);
    }

    #[test]
    fn client_reports_an_ipc_error_when_no_server_is_listening() {
        let temp_dir = tempfile::tempdir().expect("a temporary directory should be created");
        let socket_path = temp_socket_path(&temp_dir);

        let error =
            send_confirmation_request(&socket_path, &sample_request(), Duration::from_secs(1))
                .expect_err("connecting without a server must fail");

        assert!(matches!(error, GitGuardError::IpcError(_)));
    }

    #[test]
    fn server_survives_malformed_json_and_keeps_serving() {
        let temp_dir = tempfile::tempdir().expect("a temporary directory should be created");
        let socket_path = temp_socket_path(&temp_dir);
        let handler = Arc::new(ConfirmingHandler::new(Duration::ZERO));
        let calls = Arc::clone(&handler.calls);
        let server =
            IpcServer::start(socket_path.clone(), handler).expect("the IPC server should start");

        {
            let mut stream =
                UnixStream::connect(&socket_path).expect("the test client should connect");
            stream
                .write_all(b"this is not a json request\n")
                .expect("the malformed request should be written");
            // 服务端不会回包，只会关闭连接；读到空串或超时都说明它没有崩溃。
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            let mut response_line = String::new();
            let _ = BufReader::new(&stream).read_line(&mut response_line);
        }

        // 关键断言：处理过非法输入后，服务端仍然能正常响应下一次请求。
        let response =
            send_confirmation_request(&socket_path, &sample_request(), Duration::from_secs(5))
                .expect("the server should keep serving after malformed input");
        assert_eq!(response.decision, ConfirmationDecision::Confirmed);
        assert_eq!(calls.load(Ordering::Relaxed), 1);

        drop(server);
    }

    #[test]
    fn client_reports_an_ipc_error_when_the_response_times_out() {
        let temp_dir = tempfile::tempdir().expect("a temporary directory should be created");
        let socket_path = temp_socket_path(&temp_dir);
        let handler = Arc::new(ConfirmingHandler::new(Duration::from_millis(500)));
        let server =
            IpcServer::start(socket_path.clone(), handler).expect("the IPC server should start");

        let error =
            send_confirmation_request(&socket_path, &sample_request(), Duration::from_millis(100))
                .expect_err("a slow handler must surface as an IPC error, never as approval");

        assert!(matches!(error, GitGuardError::IpcError(_)));

        drop(server);
    }
}
