//! Tauri 桌面容器入口。
//!
//! 这里只提供界面展示所需的只读命令，不实现策略，也不决定 push 是否放行。

/// 稳定的开发状态标识。展示文案由前端按当前语言渲染，状态码不随语言变化。
const DEVELOPMENT_STATUS_WEEK1_CONFIRMATION_UNAVAILABLE: &str = "week1_confirmation_unavailable";

/// 前端通过 `invoke("development_status")` 调用的只读命令。
///
/// 返回稳定状态码而不是面向用户的句子，让界面语言与后端逻辑解耦；
/// 前端遇到未知状态码时必须按 fail-closed 文案展示。
#[tauri::command]
fn development_status() -> &'static str {
    DEVELOPMENT_STATUS_WEEK1_CONFIRMATION_UNAVAILABLE
}

/// 构建并启动 Tauri 应用；注册的命令名必须与前端 `invoke` 的名称一致。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![development_status])
        .run(tauri::generate_context!())
        // 启动期不可恢复错误：此时还没有窗口与日志通道，只能 panic 并打印原因。
        .expect("error while running GitGuard");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn development_status_returns_a_stable_code() {
        assert_eq!(
            development_status(),
            "week1_confirmation_unavailable",
            "前端依赖该标识映射文案，改动需同步 messages 与前端测试"
        );
    }
}
