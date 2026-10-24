#[tauri::command]
fn development_status() -> &'static str {
    "Week 1: confirmation is unavailable and therefore denied"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![development_status])
        .run(tauri::generate_context!())
        .expect("error while running GitGuard");
}
