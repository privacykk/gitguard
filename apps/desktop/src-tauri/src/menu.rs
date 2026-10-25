//! 原生菜单的构建与语言标签。
//!
//! 菜单只负责两件事：把「设置…」的快捷键交给系统，以及在触发时通知前端。
//! 它不读取配置、不判断策略；构建失败也只影响菜单本身，不影响任何确认或放行。

use tauri::menu::{Menu, MenuBuilder, MenuItem, SubmenuBuilder};
use tauri::{AppHandle, Runtime};

/// 「设置…」菜单项的稳定 ID；菜单事件据此判断是否通知前端。
pub(crate) const SETTINGS_MENU_ID: &str = "settings";

/// 前端监听的事件名：打开设置页。
pub(crate) const EVENT_OPEN_SETTINGS: &str = "open-settings";

/// macOS 通用快捷键 ⌘,；其它平台由 Tauri 映射为 Ctrl+,。
const SETTINGS_ACCELERATOR: &str = "CmdOrCtrl+,";

/// 应用菜单标题不参与翻译，与产品名保持一致。
const APP_MENU_TITLE: &str = "GitGuard";

/// 菜单语言只跟随界面语言，不参与任何安全判断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuLanguage {
    ZhCn,
    EnUs,
}

impl MenuLanguage {
    /// 把前端的 locale 收敛到受支持的语言；未知值回落到默认语言。
    pub(crate) fn from_locale(locale: &str) -> Self {
        match locale {
            "en-US" => Self::EnUs,
            _ => Self::ZhCn,
        }
    }

    fn labels(self) -> &'static MenuLabels {
        match self {
            Self::ZhCn => &ZH_CN,
            Self::EnUs => &EN_US,
        }
    }
}

/// 一套菜单文案。字段与菜单项一一对应，新增菜单项时两种语言都必须补齐。
struct MenuLabels {
    about: &'static str,
    settings: &'static str,
    services: &'static str,
    hide: &'static str,
    hide_others: &'static str,
    show_all: &'static str,
    quit: &'static str,
    edit: &'static str,
    undo: &'static str,
    redo: &'static str,
    cut: &'static str,
    copy: &'static str,
    paste: &'static str,
    select_all: &'static str,
    view: &'static str,
    fullscreen: &'static str,
    window: &'static str,
    minimize: &'static str,
    zoom: &'static str,
    close_window: &'static str,
}

const ZH_CN: MenuLabels = MenuLabels {
    about: "关于 GitGuard",
    settings: "设置…",
    services: "服务",
    hide: "隐藏 GitGuard",
    hide_others: "隐藏其他",
    show_all: "显示全部",
    quit: "退出 GitGuard",
    edit: "编辑",
    undo: "撤销",
    redo: "重做",
    cut: "剪切",
    copy: "复制",
    paste: "粘贴",
    select_all: "全选",
    view: "视图",
    fullscreen: "切换全屏",
    window: "窗口",
    minimize: "最小化",
    zoom: "缩放",
    close_window: "关闭窗口",
};

const EN_US: MenuLabels = MenuLabels {
    about: "About GitGuard",
    settings: "Settings…",
    services: "Services",
    hide: "Hide GitGuard",
    hide_others: "Hide Others",
    show_all: "Show All",
    quit: "Quit GitGuard",
    edit: "Edit",
    undo: "Undo",
    redo: "Redo",
    cut: "Cut",
    copy: "Copy",
    paste: "Paste",
    select_all: "Select All",
    view: "View",
    fullscreen: "Toggle Full Screen",
    window: "Window",
    minimize: "Minimize",
    zoom: "Zoom",
    close_window: "Close Window",
};

/// 按语言构建应用菜单；调用方负责把它设置到 AppHandle 上。
pub(crate) fn build_menu<R: Runtime>(
    handle: &AppHandle<R>,
    language: MenuLanguage,
) -> tauri::Result<Menu<R>> {
    let labels = language.labels();

    let settings = MenuItem::with_id(
        handle,
        SETTINGS_MENU_ID,
        labels.settings,
        true,
        Some(SETTINGS_ACCELERATOR),
    )?;

    let app_menu = SubmenuBuilder::new(handle, APP_MENU_TITLE)
        .about_with_text(labels.about, None)
        .separator()
        .item(&settings)
        .separator();

    // 服务、隐藏等菜单项只有 macOS 支持；其它平台直接省略。
    #[cfg(target_os = "macos")]
    let app_menu = app_menu
        .services_with_text(labels.services)
        .separator()
        .hide_with_text(labels.hide)
        .hide_others_with_text(labels.hide_others)
        .show_all_with_text(labels.show_all)
        .separator();

    let app_menu = app_menu.quit_with_text(labels.quit).build()?;

    let edit_menu = SubmenuBuilder::new(handle, labels.edit)
        .undo_with_text(labels.undo)
        .redo_with_text(labels.redo)
        .separator()
        .cut_with_text(labels.cut)
        .copy_with_text(labels.copy)
        .paste_with_text(labels.paste)
        .select_all_with_text(labels.select_all)
        .build()?;

    let view_menu = SubmenuBuilder::new(handle, labels.view)
        .fullscreen_with_text(labels.fullscreen)
        .build()?;

    // 使用 Tauri 约定的 Window 子菜单 ID，macOS 才会把它当作系统窗口菜单。
    let window_menu =
        SubmenuBuilder::with_id(handle, tauri::menu::WINDOW_SUBMENU_ID, labels.window)
            .minimize_with_text(labels.minimize)
            .maximize_with_text(labels.zoom)
            .separator()
            .close_window_with_text(labels.close_window)
            .build()?;

    MenuBuilder::new(handle)
        .item(&app_menu)
        .item(&edit_menu)
        .item(&view_menu)
        .item(&window_menu)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_follow_the_interface_language() {
        assert_eq!(
            MenuLanguage::from_locale("zh-CN").labels().settings,
            "设置…"
        );
        assert_eq!(
            MenuLanguage::from_locale("en-US").labels().settings,
            "Settings…"
        );
        assert_ne!(
            MenuLanguage::from_locale("zh-CN").labels().edit,
            MenuLanguage::from_locale("en-US").labels().edit
        );
    }

    #[test]
    fn unsupported_locales_fall_back_to_the_default_language() {
        for locale in ["", "fr-FR", "zh-TW", "EN-us"] {
            assert_eq!(MenuLanguage::from_locale(locale), MenuLanguage::ZhCn);
        }
    }
}
