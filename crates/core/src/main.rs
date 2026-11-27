#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod accounts;
mod config;
mod icon;
mod ipc;
mod notify;
mod tray;
mod win_utils;

use std::collections::HashMap;
use std::fs;
use std::time::{Duration, Instant};

use tao::dpi::{LogicalPosition, LogicalSize};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::{Window, WindowBuilder};
use wry::{Rect, WebContext, WebView, WebViewBuilder};

use crate::accounts::{sidebar_width, AccountManager};
use crate::config::AppConfig;
use crate::ipc::{AccountViewState, ToastPayload, UiCommand, UiStatePayload, UserEvent};
use crate::tray::AppTray;

const UI_HTML: &str = include_str!("ui/index.html");
const UI_CSS: &str = include_str!("ui/styles.css");
const UI_JS: &str = include_str!("ui/app.js");

fn build_combined_ui_html() -> String {
    UI_HTML
        .replace("/*__STYLES__*/", UI_CSS)
        .replace("/*__SCRIPT__*/", UI_JS)
}

fn logical_window_size(window: &Window) -> LogicalSize<f64> {
    let scale = window.scale_factor();
    let phys = window.inner_size();
    let logical = phys.to_logical::<f64>(scale);
    LogicalSize::new(logical.width.max(300.0), logical.height.max(200.0))
}

fn ui_bounds(logical_size: LogicalSize<f64>, settings_open: bool, sidebar_collapsed: bool) -> Rect {
    let w = if settings_open {
        logical_size.width
    } else {
        sidebar_width(sidebar_collapsed)
    };
    Rect {
        position: LogicalPosition::new(0.0, 0.0).into(),
        size: LogicalSize::new(w, logical_size.height).into(),
    }
}

fn update_window_title(window: &Window, cfg: &AppConfig, manager: &AccountManager) {
    let active_name = cfg
        .active_account()
        .map(|a| a.name.as_str())
        .unwrap_or("Zalo");
    let total = manager.total_unread();
    let title = if total > 0 {
        format!("({total}) Mutil-Zalo — {active_name}")
    } else {
        format!("Mutil-Zalo — {active_name}")
    };
    window.set_title(&title);
}

fn push_ui_state(
    ui_webview: &WebView,
    cfg: &AppConfig,
    manager: &AccountManager,
    settings_open: bool,
    toast: Option<ToastPayload>,
) {
    let accounts: Vec<AccountViewState> = cfg
        .accounts
        .iter()
        .map(|a| AccountViewState {
            id: a.id.clone(),
            name: a.name.clone(),
            color: a.color.clone(),
            note: a.note.clone(),
            loaded: manager.is_loaded(&a.id),
            unread_count: manager.unread_count(&a.id),
        })
        .collect();

    let payload = UiStatePayload {
        accounts,
        active_account_id: cfg.active_account_id.clone(),
        settings: cfg.settings.clone(),
        settings_open,
        is_installed: win_utils::is_installed(),
        has_desktop_shortcut: win_utils::has_desktop_shortcut(),
        install_dir: win_utils::install_dir().to_string_lossy().to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        toast,
    };

    if let Ok(json) = serde_json::to_string(&payload) {
        let script = format!("window.__mutilZaloUpdate && window.__mutilZaloUpdate({json});");
        let _ = ui_webview.evaluate_script(&script);
    }
}

fn unread_map(cfg: &AppConfig, manager: &AccountManager) -> HashMap<String, u32> {
    cfg.accounts
        .iter()
        .map(|a| (a.id.clone(), manager.unread_count(&a.id)))
        .collect()
}

fn hide_to_tray(
    window: &Window,
    window_visible: &mut bool,
    shown_tray_hint: &mut bool,
    enable_notifications: bool,
    proxy: &tao::event_loop::EventLoopProxy<UserEvent>,
) {
    window.set_visible(false);
    *window_visible = false;
    if !*shown_tray_hint && enable_notifications {
        *shown_tray_hint = true;
        notify::show_windows_toast(
            None,
            "Mutil-Zalo",
            "Đang chạy ẩn ở khay hệ thống",
            "Các tài khoản Zalo vẫn kết nối nền để nhận thông báo. Bấm biểu tượng dưới góc phải để mở lại.",
            proxy.clone(),
        );
    }
}

fn show_window(window: &Window, window_visible: &mut bool) {
    window.set_visible(true);
    window.set_minimized(false);
    window.set_focus();
    *window_visible = true;
}

fn main() {
    let _single_instance = match win_utils::acquire_single_instance() {
        Some(g) => g,
        None => return,
    };

    notify::init_notifications();

    let mut cfg = AppConfig::load_or_default();
    if cfg.settings.auto_hide_sidebar {
        cfg.settings.sidebar_collapsed = true;
    }
    let _ = cfg.save();

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let mut win_builder = WindowBuilder::new()
        .with_title("Mutil-Zalo")
        .with_inner_size(LogicalSize::new(1240.0, 800.0))
        .with_min_inner_size(LogicalSize::new(820.0, 560.0));

    if let Some(ico) = icon::window_icon() {
        win_builder = win_builder.with_window_icon(Some(ico));
    }

    let window = win_builder
        .build(&event_loop)
        .expect("Không thể tạo cửa sổ chính Mutil-Zalo");

    win_utils::save_main_window_hwnd(&window);

    let initial_size = logical_window_size(&window);

    let ui_cache_dir = AppConfig::app_data_root().join("ui-cache");
    let _ = fs::create_dir_all(&ui_cache_dir);
    let mut ui_context = WebContext::new(Some(ui_cache_dir));

    let proxy_ui = proxy.clone();
    let ui_webview = WebViewBuilder::new_with_web_context(&mut ui_context)
        .with_html(build_combined_ui_html())
        .with_bounds(ui_bounds(initial_size, false, cfg.settings.sidebar_collapsed))
        .with_ipc_handler(move |req| {
            if let Ok(cmd) = serde_json::from_str::<UiCommand>(req.body()) {
                let _ = proxy_ui.send_event(UserEvent::UiCommand(cmd));
            }
        })
        .build_as_child(&window)
        .expect("Không thể khởi tạo giao diện điều khiển Mutil-Zalo");

    let mut manager = AccountManager::new();
    let mut settings_open = false;
    let mut window_visible = true;
    let mut shown_tray_hint = false;
    let mut last_notify_time: HashMap<String, Instant> = HashMap::new();
    let mut last_notify_sig: HashMap<String, (String, Instant)> = HashMap::new();

    if cfg.settings.preload_all_accounts {
        let active_id = cfg.active_account_id.clone();
        for acc in &cfg.accounts {
            let visible = active_id.as_deref() == Some(acc.id.as_str());
            let _ = manager.ensure_session(
                &acc.id,
                &window,
                initial_size,
                cfg.settings.sidebar_collapsed,
                visible,
                cfg.settings.zoom_percent,
                proxy.clone(),
            );
        }
    } else if let Some(active_id) = cfg.active_account_id.clone() {
        let _ = manager.switch_to(
            &active_id,
            &window,
            initial_size,
            cfg.settings.sidebar_collapsed,
            false,
            cfg.settings.zoom_percent,
            proxy.clone(),
        );
    }

    update_window_title(&window, &cfg, &manager);

    let tray = AppTray::init(
        &cfg.accounts,
        cfg.active_account_id.as_deref(),
        &unread_map(&cfg, &manager),
        cfg.settings.sidebar_collapsed,
        proxy.clone(),
    )
    .ok();

    let proxy_loop = proxy.clone();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        let _keep_ui_ctx = &ui_context;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                if cfg.settings.minimize_to_tray && tray.is_some() {
                    hide_to_tray(
                        &window,
                        &mut window_visible,
                        &mut shown_tray_hint,
                        cfg.settings.enable_notifications,
                        &proxy_loop,
                    );
                } else {
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. },
                ..
            } => {
                let size = logical_window_size(&window);
                let _ = ui_webview.set_bounds(ui_bounds(
                    size,
                    settings_open,
                    cfg.settings.sidebar_collapsed,
                ));
                manager.resize_all(size, cfg.settings.sidebar_collapsed);
            }
            Event::UserEvent(user_event) => {
                let size = logical_window_size(&window);
                match user_event {
                    UserEvent::UiCommand(cmd) => match cmd {
                        UiCommand::Ready => {
                            push_ui_state(&ui_webview, &cfg, &manager, settings_open, None);
                        }
                        UiCommand::SwitchAccount { id } => {
                            if cfg.set_active(&id) {
                                if cfg.settings.auto_hide_sidebar {
                                    cfg.settings.sidebar_collapsed = true;
                                }
                                let _ = cfg.save();
                                settings_open = false;
                                let _ = ui_webview.set_bounds(ui_bounds(
                                    size,
                                    false,
                                    cfg.settings.sidebar_collapsed,
                                ));
                                let res = manager.switch_to(
                                    &id,
                                    &window,
                                    size,
                                    cfg.settings.sidebar_collapsed,
                                    false,
                                    cfg.settings.zoom_percent,
                                    proxy_loop.clone(),
                                );
                                update_window_title(&window, &cfg, &manager);
                                if let Some(t) = &tray {
                                    t.refresh(
                                        &cfg.accounts,
                                        cfg.active_account_id.as_deref(),
                                        &unread_map(&cfg, &manager),
                                        cfg.settings.sidebar_collapsed,
                                    );
                                }
                                let toast = res.err().map(|m| ToastPayload {
                                    kind: "error".into(),
                                    message: m,
                                });
                                push_ui_state(&ui_webview, &cfg, &manager, settings_open, toast);
                            }
                        }
                        UiCommand::AddAccount { name, note, color } => {
                            let acc = cfg.add_account(name, note, color);
                            if cfg.settings.auto_hide_sidebar {
                                cfg.settings.sidebar_collapsed = true;
                            }
                            let _ = cfg.save();
                            settings_open = false;
                            let _ = ui_webview.set_bounds(ui_bounds(
                                size,
                                false,
                                cfg.settings.sidebar_collapsed,
                            ));
                            let res = manager.switch_to(
                                &acc.id,
                                &window,
                                size,
                                cfg.settings.sidebar_collapsed,
                                false,
                                cfg.settings.zoom_percent,
                                proxy_loop.clone(),
                            );
                            update_window_title(&window, &cfg, &manager);
                            if let Some(t) = &tray {
                                t.refresh(
                                    &cfg.accounts,
                                    cfg.active_account_id.as_deref(),
                                    &unread_map(&cfg, &manager),
                                    cfg.settings.sidebar_collapsed,
                                );
                            }
                            let toast = match res {
                                Ok(()) => Some(ToastPayload {
                                    kind: "success".into(),
                                    message: format!("Đã thêm tài khoản \"{}\"", acc.name),
                                }),
                                Err(e) => Some(ToastPayload {
                                    kind: "error".into(),
                                    message: e,
                                }),
                            };
                            push_ui_state(&ui_webview, &cfg, &manager, settings_open, toast);
                        }
                        UiCommand::UpdateAccount {
                            id,
                            name,
                            note,
                            color,
                        } => {
                            if cfg.update_account(&id, name, note, color) {
                                let _ = cfg.save();
                                update_window_title(&window, &cfg, &manager);
                                if let Some(t) = &tray {
                                    t.refresh(
                                        &cfg.accounts,
                                        cfg.active_account_id.as_deref(),
                                        &unread_map(&cfg, &manager),
                                        cfg.settings.sidebar_collapsed,
                                    );
                                }
                                push_ui_state(
                                    &ui_webview,
                                    &cfg,
                                    &manager,
                                    settings_open,
                                    Some(ToastPayload {
                                        kind: "success".into(),
                                        message: "Đã cập nhật thông tin tài khoản.".into(),
                                    }),
                                );
                            }
                        }
                        UiCommand::DeleteAccount { id } => match cfg.remove_account(&id) {
                            Ok(removed) => {
                                let _ = cfg.save();
                                manager.drop_and_delete_profile(&removed.id);
                                if let Some(next_id) = cfg.active_account_id.clone() {
                                    let _ = manager.switch_to(
                                        &next_id,
                                        &window,
                                        size,
                                        cfg.settings.sidebar_collapsed,
                                        settings_open,
                                        cfg.settings.zoom_percent,
                                        proxy_loop.clone(),
                                    );
                                }
                                update_window_title(&window, &cfg, &manager);
                                if let Some(t) = &tray {
                                    t.refresh(
                                        &cfg.accounts,
                                        cfg.active_account_id.as_deref(),
                                        &unread_map(&cfg, &manager),
                                        cfg.settings.sidebar_collapsed,
                                    );
                                }
                                push_ui_state(
                                    &ui_webview,
                                    &cfg,
                                    &manager,
                                    settings_open,
                                    Some(ToastPayload {
                                        kind: "info".into(),
                                        message: format!("Đã xóa tài khoản \"{}\".", removed.name),
                                    }),
                                );
                            }
                            Err(msg) => {
                                push_ui_state(
                                    &ui_webview,
                                    &cfg,
                                    &manager,
                                    settings_open,
                                    Some(ToastPayload {
                                        kind: "error".into(),
                                        message: msg.to_string(),
                                    }),
                                );
                            }
                        },
                        UiCommand::ReloadAccount { id } => {
                            if manager.is_loaded(&id) {
                                manager.reload_account(&id);
                            } else {
                                let _ = manager.ensure_session(
                                    &id,
                                    &window,
                                    size,
                                    cfg.settings.sidebar_collapsed,
                                    !settings_open
                                        && cfg.active_account_id.as_deref() == Some(id.as_str()),
                                    cfg.settings.zoom_percent,
                                    proxy_loop.clone(),
                                );
                            }
                            push_ui_state(
                                &ui_webview,
                                &cfg,
                                &manager,
                                settings_open,
                                Some(ToastPayload {
                                    kind: "info".into(),
                                    message: "Đang tải lại trang Zalo Web...".into(),
                                }),
                            );
                        }
                        UiCommand::ClearAccountSession { id } => {
                            manager.drop_and_delete_profile(&id);
                            if cfg.active_account_id.as_deref() == Some(id.as_str()) {
                                let _ = manager.switch_to(
                                    &id,
                                    &window,
                                    size,
                                    cfg.settings.sidebar_collapsed,
                                    settings_open,
                                    cfg.settings.zoom_percent,
                                    proxy_loop.clone(),
                                );
                            }
                            push_ui_state(
                                &ui_webview,
                                &cfg,
                                &manager,
                                settings_open,
                                Some(ToastPayload {
                                    kind: "info".into(),
                                    message: "Đã xóa phiên đăng nhập của tài khoản này.".into(),
                                }),
                            );
                        }
                        UiCommand::SetSettingsOpen { open } => {
                            settings_open = open;
                            if settings_open {
                                manager.hide_all();
                                let _ = ui_webview.set_bounds(ui_bounds(
                                    size,
                                    true,
                                    cfg.settings.sidebar_collapsed,
                                ));
                                let _ = ui_webview.focus();
                            } else {
                                if cfg.settings.auto_hide_sidebar {
                                    cfg.settings.sidebar_collapsed = true;
                                }
                                let _ = ui_webview.set_bounds(ui_bounds(
                                    size,
                                    false,
                                    cfg.settings.sidebar_collapsed,
                                ));
                                manager.show_active(
                                    cfg.active_account_id.as_deref(),
                                    size,
                                    cfg.settings.sidebar_collapsed,
                                );
                            }
                            push_ui_state(&ui_webview, &cfg, &manager, settings_open, None);
                        }
                        UiCommand::ToggleSidebar => {
                            let _ = proxy_loop.send_event(UserEvent::ToggleSidebar);
                        }
                        UiCommand::MinimizeToTray => {
                            if tray.is_some() {
                                hide_to_tray(
                                    &window,
                                    &mut window_visible,
                                    &mut shown_tray_hint,
                                    cfg.settings.enable_notifications,
                                    &proxy_loop,
                                );
                            } else {
                                window.set_minimized(true);
                            }
                        }
                        UiCommand::UpdateSettings { mut settings } => {
                            settings.zoom_percent = settings.zoom_percent.clamp(75, 150);
                            let mut toast = Some(ToastPayload {
                                kind: "success".into(),
                                message: "Đã lưu cài đặt.".into(),
                            });

                            if settings.start_with_windows != cfg.settings.start_with_windows {
                                if let Err(e) =
                                    win_utils::set_start_with_windows(settings.start_with_windows)
                                {
                                    settings.start_with_windows = cfg.settings.start_with_windows;
                                    toast = Some(ToastPayload {
                                        kind: "error".into(),
                                        message: e,
                                    });
                                }
                            }

                            if settings.zoom_percent != cfg.settings.zoom_percent {
                                manager.apply_zoom_all(settings.zoom_percent);
                            }

                            if settings.auto_hide_sidebar != cfg.settings.auto_hide_sidebar {
                                settings.sidebar_collapsed = settings.auto_hide_sidebar;
                            }

                            let collapsed_changed =
                                settings.sidebar_collapsed != cfg.settings.sidebar_collapsed;

                            if settings.preload_all_accounts && !cfg.settings.preload_all_accounts {
                                for acc in &cfg.accounts {
                                    let visible = !settings_open
                                        && cfg.active_account_id.as_deref()
                                            == Some(acc.id.as_str());
                                    let _ = manager.ensure_session(
                                        &acc.id,
                                        &window,
                                        size,
                                        settings.sidebar_collapsed,
                                        visible,
                                        settings.zoom_percent,
                                        proxy_loop.clone(),
                                    );
                                }
                            }

                            cfg.settings = settings;
                            let _ = cfg.save();

                            if collapsed_changed {
                                let _ = ui_webview.set_bounds(ui_bounds(
                                    size,
                                    settings_open,
                                    cfg.settings.sidebar_collapsed,
                                ));
                                manager.resize_all(size, cfg.settings.sidebar_collapsed);
                                if let Some(t) = &tray {
                                    t.refresh(
                                        &cfg.accounts,
                                        cfg.active_account_id.as_deref(),
                                        &unread_map(&cfg, &manager),
                                        cfg.settings.sidebar_collapsed,
                                    );
                                }
                            }

                            push_ui_state(&ui_webview, &cfg, &manager, settings_open, toast);
                        }
                        UiCommand::TestNotification => {
                            let active_id = cfg.active_account_id.clone();
                            let active_name = cfg
                                .active_account()
                                .map(|a| a.name.clone())
                                .unwrap_or_else(|| "Zalo".to_string());
                            notify::show_windows_toast(
                                active_id,
                                &active_name,
                                "Kiểm tra thông báo",
                                "Thông báo Mutil-Zalo hoạt động bình thường! Bấm vào đây để mở nhanh cửa sổ.",
                                proxy_loop.clone(),
                            );
                            push_ui_state(
                                &ui_webview,
                                &cfg,
                                &manager,
                                settings_open,
                                Some(ToastPayload {
                                    kind: "info".into(),
                                    message: "Đã gửi thông báo thử ra Windows.".into(),
                                }),
                            );
                        }
                        UiCommand::OpenDataFolder => {
                            let dir = AppConfig::app_data_root();
                            let _ = fs::create_dir_all(&dir);
                            win_utils::open_external(&dir.to_string_lossy());
                        }
                        UiCommand::BrowseInstallDir { current_dir } => {
                            let start_dir = current_dir
                                .as_deref()
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(std::path::PathBuf::from)
                                .unwrap_or_else(win_utils::install_dir);
                            if let Some(picked) = win_utils::pick_install_folder(&window, &start_dir)
                            {
                                if let Ok(json) =
                                    serde_json::to_string(&picked.to_string_lossy().to_string())
                                {
                                    let script = format!(
                                        "window.__mutilZaloSetInstallDir && window.__mutilZaloSetInstallDir({json});"
                                    );
                                    let _ = ui_webview.evaluate_script(&script);
                                }
                            }
                        }
                        UiCommand::InstallToWindows {
                            install_dir,
                            create_desktop_shortcut,
                        } => {
                            let custom_dir = install_dir
                                .as_deref()
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(std::path::PathBuf::from);
                            let opts = win_utils::InstallOptions {
                                install_dir: custom_dir,
                                create_desktop_shortcut,
                                create_start_menu_shortcut: true,
                            };
                            let toast = match win_utils::install_to_windows(opts) {
                                Ok(msg) => {
                                    if cfg.settings.start_with_windows {
                                        let _ = win_utils::set_start_with_windows(true);
                                    }
                                    ToastPayload {
                                        kind: "success".into(),
                                        message: msg,
                                    }
                                }
                                Err(e) => ToastPayload {
                                    kind: "error".into(),
                                    message: e,
                                },
                            };
                            push_ui_state(&ui_webview, &cfg, &manager, settings_open, Some(toast));
                        }
                        UiCommand::CreateDesktopShortcut => {
                            let toast = match win_utils::create_desktop_shortcut_now() {
                                Ok(msg) => ToastPayload {
                                    kind: "success".into(),
                                    message: msg,
                                },
                                Err(e) => ToastPayload {
                                    kind: "error".into(),
                                    message: e,
                                },
                            };
                            push_ui_state(&ui_webview, &cfg, &manager, settings_open, Some(toast));
                        }
                        UiCommand::UninstallFromWindows => {
                            let toast = match win_utils::uninstall_from_windows() {
                                Ok(msg) => {
                                    cfg.settings.start_with_windows = false;
                                    let _ = cfg.save();
                                    ToastPayload {
                                        kind: "info".into(),
                                        message: msg,
                                    }
                                }
                                Err(e) => ToastPayload {
                                    kind: "error".into(),
                                    message: e,
                                },
                            };
                            push_ui_state(&ui_webview, &cfg, &manager, settings_open, Some(toast));
                        }
                    },
                    UserEvent::ZaloNotify {
                        account_id,
                        title,
                        body,
                    } => {
                        if cfg.settings.enable_notifications {
                            let now = Instant::now();
                            let sig = format!("{title}|{body}");
                            let is_dup = last_notify_sig
                                .get(&account_id)
                                .map(|(prev_sig, t)| {
                                    prev_sig == &sig && now.duration_since(*t) < Duration::from_millis(800)
                                })
                                .unwrap_or(false);

                            if !is_dup {
                                last_notify_sig.insert(account_id.clone(), (sig, now));
                                last_notify_time.insert(account_id.clone(), now);

                                let acc_name = cfg
                                    .accounts
                                    .iter()
                                    .find(|a| a.id == account_id)
                                    .map(|a| a.name.clone())
                                    .unwrap_or_else(|| "Zalo".to_string());

                                notify::show_windows_toast(
                                    Some(account_id),
                                    &acc_name,
                                    &title,
                                    &body,
                                    proxy_loop.clone(),
                                );
                                win_utils::flash_taskbar_if_unfocused();
                            }
                        }
                    }
                    UserEvent::ZaloUnread { account_id, count } => {
                        let (old_count, changed) = manager.set_unread(&account_id, count);
                        if changed {
                            update_window_title(&window, &cfg, &manager);
                            if let Some(t) = &tray {
                                t.refresh(
                                    &cfg.accounts,
                                    cfg.active_account_id.as_deref(),
                                    &unread_map(&cfg, &manager),
                                    cfg.settings.sidebar_collapsed,
                                );
                            }
                            push_ui_state(&ui_webview, &cfg, &manager, settings_open, None);

                            // Fallback Windows Toast when unread count rises and Web Notification didn't already fire
                            if count > old_count && cfg.settings.enable_notifications {
                                let recently_notified = last_notify_time
                                    .get(&account_id)
                                    .map(|t| t.elapsed() < Duration::from_secs(2))
                                    .unwrap_or(false);

                                if !recently_notified {
                                    last_notify_time.insert(account_id.clone(), Instant::now());
                                    let acc_name = cfg
                                        .accounts
                                        .iter()
                                        .find(|a| a.id == account_id)
                                        .map(|a| a.name.clone())
                                        .unwrap_or_else(|| "Zalo".to_string());
                                    notify::show_windows_toast(
                                        Some(account_id),
                                        &acc_name,
                                        "Tin nhắn mới",
                                        &format!("Bạn có {count} tin nhắn chưa đọc trên {acc_name}"),
                                        proxy_loop.clone(),
                                    );
                                    win_utils::flash_taskbar_if_unfocused();
                                }
                            }
                        }
                    }
                    UserEvent::ShortcutSwitch(idx) => {
                        if let Some(acc) = cfg.accounts.get(idx).cloned() {
                            let _ = proxy_loop
                                .send_event(UserEvent::UiCommand(UiCommand::SwitchAccount {
                                    id: acc.id,
                                }));
                        }
                    }
                    UserEvent::ToggleSettings => {
                        let _ = proxy_loop.send_event(UserEvent::UiCommand(
                            UiCommand::SetSettingsOpen {
                                open: !settings_open,
                            },
                        ));
                    }
                    UserEvent::ToggleSidebar => {
                        cfg.settings.sidebar_collapsed = !cfg.settings.sidebar_collapsed;
                        let _ = cfg.save();
                        let _ = ui_webview.set_bounds(ui_bounds(
                            size,
                            settings_open,
                            cfg.settings.sidebar_collapsed,
                        ));
                        manager.resize_all(size, cfg.settings.sidebar_collapsed);
                        if let Some(t) = &tray {
                            t.refresh(
                                &cfg.accounts,
                                cfg.active_account_id.as_deref(),
                                &unread_map(&cfg, &manager),
                                cfg.settings.sidebar_collapsed,
                            );
                        }
                        push_ui_state(&ui_webview, &cfg, &manager, settings_open, None);
                    }
                    UserEvent::TrayShow => {
                        show_window(&window, &mut window_visible);
                    }
                    UserEvent::TrayToggleWindow => {
                        if window_visible && !window.is_minimized() {
                            hide_to_tray(
                                &window,
                                &mut window_visible,
                                &mut shown_tray_hint,
                                cfg.settings.enable_notifications,
                                &proxy_loop,
                            );
                        } else {
                            show_window(&window, &mut window_visible);
                        }
                    }
                    UserEvent::TrayOpenSettings => {
                        show_window(&window, &mut window_visible);
                        let _ = proxy_loop.send_event(UserEvent::UiCommand(
                            UiCommand::SetSettingsOpen { open: true },
                        ));
                    }
                    UserEvent::TraySwitchAccount(id) => {
                        show_window(&window, &mut window_visible);
                        let _ = proxy_loop
                            .send_event(UserEvent::UiCommand(UiCommand::SwitchAccount { id }));
                    }
                    UserEvent::TrayQuit => {
                        *control_flow = ControlFlow::Exit;
                    }
                }
            }
            _ => {}
        }
    });
}
