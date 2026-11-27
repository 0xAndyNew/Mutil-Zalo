use std::collections::HashMap;
use std::sync::Arc;

use tao::event_loop::EventLoopProxy;
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use crate::config::Account;
use crate::icon;
use crate::ipc::UserEvent;

const ID_TOGGLE_WINDOW: &str = "tray_toggle_window";
const ID_TOGGLE_SIDEBAR: &str = "tray_toggle_sidebar";
const ID_SETTINGS: &str = "tray_settings";
const ID_QUIT: &str = "tray_quit";
const ACC_PREFIX: &str = "tray_acc::";

pub struct AppTray {
    tray: TrayIcon,
}

impl AppTray {
    pub fn init(
        accounts: &[Account],
        active_id: Option<&str>,
        unread: &HashMap<String, u32>,
        sidebar_collapsed: bool,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Result<Self, String> {
        let menu = build_menu(accounts, active_id, unread, sidebar_collapsed);
        let mut builder = TrayIconBuilder::new()
            .with_tooltip("Mutil-Zalo - Zalo Web Đa Tài Khoản")
            .with_menu(Box::new(menu));

        if let Some(img) = icon::tray_icon() {
            builder = builder.with_icon(img);
        }

        let tray = builder
            .build()
            .map_err(|e| format!("Không thể tạo System Tray: {e}"))?;

        let proxy_click = proxy.clone();
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let _ = proxy_click.send_event(UserEvent::TrayShow);
            }
        }));

        MenuEvent::set_event_handler(Some(Arc::new(move |event: MenuEvent| {
            let id = event.id.0.as_str();
            if id == ID_TOGGLE_WINDOW {
                let _ = proxy.send_event(UserEvent::TrayToggleWindow);
            } else if id == ID_TOGGLE_SIDEBAR {
                let _ = proxy.send_event(UserEvent::ToggleSidebar);
            } else if id == ID_SETTINGS {
                let _ = proxy.send_event(UserEvent::TrayOpenSettings);
            } else if id == ID_QUIT {
                let _ = proxy.send_event(UserEvent::TrayQuit);
            } else if let Some(acc_id) = id.strip_prefix(ACC_PREFIX) {
                let _ = proxy.send_event(UserEvent::TraySwitchAccount(acc_id.to_string()));
            }
        })));

        Ok(Self { tray })
    }

    pub fn refresh(
        &self,
        accounts: &[Account],
        active_id: Option<&str>,
        unread: &HashMap<String, u32>,
        sidebar_collapsed: bool,
    ) {
        let menu = build_menu(accounts, active_id, unread, sidebar_collapsed);
        self.tray.set_menu(Some(Box::new(menu)));
        let total: u32 = unread.values().sum();
        let tip = if total > 0 {
            format!("Mutil-Zalo ({total} tin nhắn chưa đọc)")
        } else {
            "Mutil-Zalo - Zalo Web Đa Tài Khoản".to_string()
        };
        let _ = self.tray.set_tooltip(Some(tip));
    }
}

fn build_menu(
    accounts: &[Account],
    active_id: Option<&str>,
    unread: &HashMap<String, u32>,
    sidebar_collapsed: bool,
) -> Menu {
    let menu = Menu::new();
    let _ = menu.append(&MenuItem::with_id(
        MenuId::new(ID_TOGGLE_WINDOW),
        "Mở / Ẩn cửa sổ Mutil-Zalo",
        true,
        None,
    ));
    let sidebar_label = if sidebar_collapsed {
        "Hiện thanh chuyển tài khoản (Ctrl+B)"
    } else {
        "Thu gọn thanh chuyển tài khoản (Ctrl+B)"
    };
    let _ = menu.append(&MenuItem::with_id(
        MenuId::new(ID_TOGGLE_SIDEBAR),
        sidebar_label,
        true,
        None,
    ));
    let _ = menu.append(&PredefinedMenuItem::separator());

    for acc in accounts {
        let is_active = active_id == Some(acc.id.as_str());
        let u = unread.get(&acc.id).copied().unwrap_or(0);
        let prefix = if is_active { "● " } else { "○ " };
        let suffix = if u > 0 { format!("  ({u})") } else { String::new() };
        let label = format!("{prefix}{}{suffix}", acc.name);
        let item = MenuItem::with_id(
            MenuId::new(format!("{ACC_PREFIX}{}", acc.id)),
            label,
            true,
            None,
        );
        let _ = menu.append(&item);
    }

    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id(
        MenuId::new(ID_SETTINGS),
        "Cài đặt & Chuyển đổi tài khoản",
        true,
        None,
    ));
    let _ = menu.append(&MenuItem::with_id(
        MenuId::new(ID_QUIT),
        "Thoát hoàn toàn",
        true,
        None,
    ));
    menu
}
