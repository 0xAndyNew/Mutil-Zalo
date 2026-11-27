use serde::{Deserialize, Serialize};

use crate::config::AppSettings;

#[derive(Debug, Clone)]
pub enum UserEvent {
    UiCommand(UiCommand),
    ZaloUnread { account_id: String, count: u32 },
    ZaloNotify { account_id: String, title: String, body: String },
    ShortcutSwitch(usize),
    ToggleSettings,
    ToggleSidebar,
    TrayShow,
    TrayToggleWindow,
    TrayOpenSettings,
    TraySwitchAccount(String),
    TrayQuit,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiCommand {
    Ready,
    SwitchAccount { id: String },
    AddAccount { name: String, note: String, color: String },
    UpdateAccount { id: String, name: String, note: String, color: String },
    DeleteAccount { id: String },
    ReloadAccount { id: String },
    ClearAccountSession { id: String },
    SetSettingsOpen { open: bool },
    ToggleSidebar,
    MinimizeToTray,
    UpdateSettings { settings: AppSettings },
    TestNotification,
    OpenDataFolder,
    BrowseInstallDir {
        #[serde(default)]
        current_dir: Option<String>,
    },
    InstallToWindows {
        #[serde(default)]
        install_dir: Option<String>,
        #[serde(default = "default_true")]
        create_desktop_shortcut: bool,
    },
    CreateDesktopShortcut,
    UninstallFromWindows,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ZaloIpcMessage {
    ZaloUnread { count: u32 },
    ZaloNotify { title: String, body: String },
    ShortcutSwitch { index: usize },
    ToggleSettings,
    ToggleSidebar,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountViewState {
    pub id: String,
    pub name: String,
    pub color: String,
    pub note: String,
    pub loaded: bool,
    pub unread_count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToastPayload {
    pub kind: String, // "success" | "info" | "error"
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct UiStatePayload {
    pub accounts: Vec<AccountViewState>,
    pub active_account_id: Option<String>,
    pub settings: AppSettings,
    pub settings_open: bool,
    pub is_installed: bool,
    pub has_desktop_shortcut: bool,
    pub install_dir: String,
    pub app_version: String,
    pub toast: Option<ToastPayload>,
}
