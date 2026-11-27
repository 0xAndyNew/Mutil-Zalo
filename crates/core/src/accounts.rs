use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use tao::dpi::{LogicalPosition, LogicalSize};
use tao::event_loop::EventLoopProxy;
use tao::window::Window;
#[cfg(windows)]
use wry::{WebViewBuilderExtWindows, WebViewExtWindows};
use wry::{NewWindowResponse, Rect, WebContext, WebView, WebViewBuilder};

use crate::config::AppConfig;
use crate::ipc::{UserEvent, ZaloIpcMessage};
use crate::win_utils;

pub const SIDEBAR_WIDTH_EXPANDED: f64 = 68.0;
pub const SIDEBAR_WIDTH_COLLAPSED: f64 = 16.0;
pub const ZALO_URL: &str = "https://chat.zalo.me";

pub fn sidebar_width(collapsed: bool) -> f64 {
    if collapsed {
        SIDEBAR_WIDTH_COLLAPSED
    } else {
        SIDEBAR_WIDTH_EXPANDED
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BoundsKey {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

impl BoundsKey {
    fn from_layout(logical_size: LogicalSize<f64>, sidebar_collapsed: bool) -> Self {
        let left = sidebar_width(sidebar_collapsed);
        let w = (logical_size.width - left).max(120.0);
        let h = logical_size.height.max(120.0);
        Self {
            x: left.round() as i32,
            y: 0,
            w: w.round() as i32,
            h: h.round() as i32,
        }
    }
}

#[cfg(windows)]
fn webview_child_hwnd(webview: &WebView) -> Option<windows_sys::Win32::Foundation::HWND> {
    let mut hwnd = windows::Win32::Foundation::HWND::default();
    let ok = unsafe { webview.controller().ParentWindow(&mut hwnd) }.is_ok();
    if ok && !hwnd.0.is_null() {
        Some(hwnd.0)
    } else {
        None
    }
}

/// Switches visibility using ONLY the Win32 child wrapper HWND (ShowWindow + HWND_TOP)
/// WITHOUT calling `ICoreWebView2Controller::SetIsVisible(false)`.
/// This keeps Chromium's compositor warm and prevents Zalo Web from receiving
/// `visibilitychange` / suspend events that trigger reloads or blank flashes when switching accounts.
fn set_webview_hwnd_visible(webview: &WebView, visible: bool) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, ShowWindow, HWND_TOP, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE,
            SW_SHOW,
        };
        if let Some(hwnd) = webview_child_hwnd(webview) {
            unsafe {
                if visible {
                    ShowWindow(hwnd, SW_SHOW);
                    SetWindowPos(
                        hwnd,
                        HWND_TOP,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                    );
                } else {
                    ShowWindow(hwnd, SW_HIDE);
                }
            }
            return;
        }
    }
    let _ = webview.set_visible(visible);
}

pub struct AccountSession {
    // Kept alive for the lifetime of the child WebView2 environment.
    _web_context: WebContext,
    pub webview: WebView,
    pub unread_count: u32,
    visible: bool,
    last_bounds: BoundsKey,
    last_zoom: u32,
}

pub struct AccountManager {
    sessions: HashMap<String, AccountSession>,
}

impl AccountManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub fn is_loaded(&self, account_id: &str) -> bool {
        self.sessions.contains_key(account_id)
    }

    pub fn unread_count(&self, account_id: &str) -> u32 {
        self.sessions
            .get(account_id)
            .map(|s| s.unread_count)
            .unwrap_or(0)
    }

    pub fn total_unread(&self) -> u32 {
        self.sessions.values().map(|s| s.unread_count).sum()
    }

    /// Updates unread count and returns `(old_count, changed)`.
    pub fn set_unread(&mut self, account_id: &str, count: u32) -> (u32, bool) {
        if let Some(session) = self.sessions.get_mut(account_id) {
            let old = session.unread_count;
            if old != count {
                session.unread_count = count;
                return (old, true);
            }
            return (old, false);
        }
        (0, false)
    }

    pub fn content_bounds(logical_size: LogicalSize<f64>, sidebar_collapsed: bool) -> Rect {
        let left = sidebar_width(sidebar_collapsed);
        let w = (logical_size.width - left).max(120.0);
        let h = logical_size.height.max(120.0);
        Rect {
            position: LogicalPosition::new(left, 0.0).into(),
            size: LogicalSize::new(w, h).into(),
        }
    }

    /// Creates an isolated WebView2 session for the account if not already running.
    pub fn ensure_session(
        &mut self,
        account_id: &str,
        window: &Window,
        logical_size: LogicalSize<f64>,
        sidebar_collapsed: bool,
        visible: bool,
        zoom_percent: u32,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Result<(), String> {
        if self.sessions.contains_key(account_id) {
            return Ok(());
        }

        let profile_dir = AppConfig::profile_dir(account_id);
        let _ = fs::create_dir_all(&profile_dir);

        let mut web_context = WebContext::new(Some(profile_dir));
        let bounds = Self::content_bounds(logical_size, sidebar_collapsed);
        let bounds_key = BoundsKey::from_layout(logical_size, sidebar_collapsed);
        let acc_id_for_ipc = account_id.to_string();
        let proxy_for_ipc = proxy.clone();

        let init_script = r#"
(function() {
  if (window.__mutilZaloHooked) return;
  window.__mutilZaloHooked = true;

  function post(obj) {
    try {
      window.ipc.postMessage(JSON.stringify(obj));
    } catch (_) {}
  }

  // 0) Keep page visibility always 'visible' so switching accounts NEVER triggers
  //    Zalo Web's visibilitychange reload/resync overlay or background suspension.
  try {
    Object.defineProperty(document, 'hidden', { get: () => false, configurable: true });
    Object.defineProperty(document, 'webkitHidden', { get: () => false, configurable: true });
    Object.defineProperty(document, 'visibilityState', { get: () => 'visible', configurable: true });
    Object.defineProperty(document, 'webkitVisibilityState', { get: () => 'visible', configurable: true });
    const blockVis = (e) => {
      e.stopImmediatePropagation();
    };
    document.addEventListener('visibilitychange', blockVis, true);
    document.addEventListener('webkitvisibilitychange', blockVis, true);
    window.addEventListener('visibilitychange', blockVis, true);
  } catch (_) {}

  // 1) Bridge HTML5 Web Notification API -> Native Windows Toast Notifications
  try {
    function MutilZaloNotification(title, options) {
      const opts = options || {};
      this.title = String(title || 'Zalo');
      this.body = String(opts.body || '');
      this.tag = opts.tag || '';
      this.icon = opts.icon || '';
      this.onclick = null;
      this.onclose = null;
      this.onerror = null;
      this.onshow = null;
      post({
        type: 'zalo_notify',
        title: this.title,
        body: this.body
      });
    }
    MutilZaloNotification.permission = 'granted';
    MutilZaloNotification.requestPermission = function(cb) {
      if (typeof cb === 'function') {
        try { cb('granted'); } catch (_) {}
      }
      return Promise.resolve('granted');
    };
    MutilZaloNotification.prototype.close = function() {};
    MutilZaloNotification.prototype.addEventListener = function() {};
    MutilZaloNotification.prototype.removeEventListener = function() {};

    Object.defineProperty(window, 'Notification', {
      value: MutilZaloNotification,
      configurable: true,
      writable: true
    });

    if (window.ServiceWorkerRegistration && ServiceWorkerRegistration.prototype) {
      const origShow = ServiceWorkerRegistration.prototype.showNotification;
      ServiceWorkerRegistration.prototype.showNotification = function(title, options) {
        const opts = options || {};
        post({
          type: 'zalo_notify',
          title: String(title || 'Zalo'),
          body: String(opts.body || '')
        });
        if (typeof origShow === 'function') {
          try { return origShow.call(this, title, options); } catch (_) {}
        }
        return Promise.resolve();
      };
    }

    if (navigator.permissions && navigator.permissions.query) {
      const origQuery = navigator.permissions.query.bind(navigator.permissions);
      navigator.permissions.query = function(desc) {
        if (desc && desc.name === 'notifications') {
          return Promise.resolve({ state: 'granted', onchange: null });
        }
        return origQuery(desc);
      };
    }
  } catch (_) {}

  // 2) Observe document.title for unread badge count: "(5) Zalo"
  let lastCount = -1;
  function parseUnread(title) {
    if (!title) return 0;
    const m = title.trim().match(/^\((\d+)\+?\)/);
    return m ? parseInt(m[1], 10) || 0 : 0;
  }
  function reportUnread() {
    try {
      const count = parseUnread(document.title);
      if (count !== lastCount) {
        lastCount = count;
        post({ type: 'zalo_unread', count });
      }
    } catch (_) {}
  }

  const startObserver = () => {
    reportUnread();
    const titleEl = document.querySelector('title');
    if (titleEl && window.MutationObserver) {
      new MutationObserver(reportUnread).observe(titleEl, { childList: true, characterData: true, subtree: true });
    }
    setInterval(reportUnread, 2000);
  };

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', startObserver);
  } else {
    startObserver();
  }

  // 3) Keyboard shortcuts inside Zalo Web: Ctrl+1..9, Ctrl+,, Ctrl+B
  window.addEventListener('keydown', function(e) {
    if (e.ctrlKey && !e.altKey && !e.shiftKey) {
      if (e.key >= '1' && e.key <= '9') {
        e.preventDefault();
        post({ type: 'shortcut_switch', index: parseInt(e.key, 10) - 1 });
      } else if (e.key === ',') {
        e.preventDefault();
        post({ type: 'toggle_settings' });
      } else if (e.key === 'b' || e.key === 'B') {
        e.preventDefault();
        post({ type: 'toggle_sidebar' });
      }
    }
  }, true);
})();
"#;

        // Always create with WebView2 controller IsVisible = true so the renderer stays warm;
        // if initially hidden, hide only the Win32 child HWND right after creation.
        let mut builder = WebViewBuilder::new_with_web_context(&mut web_context)
            .with_url(ZALO_URL)
            .with_bounds(bounds)
            .with_visible(true)
            .with_autoplay(true)
            .with_clipboard(true)
            .with_initialization_script(init_script)
            .with_ipc_handler(move |req| {
                if let Ok(msg) = serde_json::from_str::<ZaloIpcMessage>(req.body()) {
                    match msg {
                        ZaloIpcMessage::ZaloUnread { count } => {
                            let _ = proxy_for_ipc.send_event(UserEvent::ZaloUnread {
                                account_id: acc_id_for_ipc.clone(),
                                count,
                            });
                        }
                        ZaloIpcMessage::ZaloNotify { title, body } => {
                            let _ = proxy_for_ipc.send_event(UserEvent::ZaloNotify {
                                account_id: acc_id_for_ipc.clone(),
                                title,
                                body,
                            });
                        }
                        ZaloIpcMessage::ShortcutSwitch { index } => {
                            let _ = proxy_for_ipc.send_event(UserEvent::ShortcutSwitch(index));
                        }
                        ZaloIpcMessage::ToggleSettings => {
                            let _ = proxy_for_ipc.send_event(UserEvent::ToggleSettings);
                        }
                        ZaloIpcMessage::ToggleSidebar => {
                            let _ = proxy_for_ipc.send_event(UserEvent::ToggleSidebar);
                        }
                    }
                }
            })
            .with_new_window_req_handler(|url, _features| {
                if win_utils::is_zalo_internal_url(&url) {
                    NewWindowResponse::Allow
                } else {
                    win_utils::open_external(&url);
                    NewWindowResponse::Deny
                }
            })
            .with_download_started_handler(|_url, dest: &mut PathBuf| {
                if let Some(downloads_dir) = dirs::download_dir() {
                    let filename = dest
                        .file_name()
                        .map(|n| n.to_os_string())
                        .unwrap_or_else(|| "zalo-file".into());
                    *dest = unique_download_path(&downloads_dir, &PathBuf::from(filename));
                }
                true
            });

        // Prevent WebView2 from suspending/occluding background Zalo tabs
        #[cfg(windows)]
        {
            builder = builder.with_additional_browser_args(
                "--disable-background-timer-throttling --disable-renderer-backgrounding --disable-backgrounding-occluded-windows --disable-features=CalculateNativeWinOcclusion,IntensiveWakeUpThrottling,BackForwardCache",
            );
        }

        let webview = builder
            .build_as_child(window)
            .map_err(|e| format!("Không thể khởi tạo WebView Zalo: {e}"))?;

        if zoom_percent != 100 {
            let _ = webview.zoom((zoom_percent as f64) / 100.0);
        }

        if !visible {
            set_webview_hwnd_visible(&webview, false);
        }

        self.sessions.insert(
            account_id.to_string(),
            AccountSession {
                _web_context: web_context,
                webview,
                unread_count: 0,
                visible,
                last_bounds: bounds_key,
                last_zoom: zoom_percent,
            },
        );
        Ok(())
    }

    /// Seamlessly switches to the target account WITHOUT logging out, reloading, or triggering resize/zoom churn.
    pub fn switch_to(
        &mut self,
        account_id: &str,
        window: &Window,
        logical_size: LogicalSize<f64>,
        sidebar_collapsed: bool,
        settings_open: bool,
        zoom_percent: u32,
        proxy: EventLoopProxy<UserEvent>,
    ) -> Result<(), String> {
        // Ensure target session exists BEFORE hiding previous session so switch feels instantaneous
        self.ensure_session(
            account_id,
            window,
            logical_size,
            sidebar_collapsed,
            !settings_open,
            zoom_percent,
            proxy,
        )?;

        let bounds_key = BoundsKey::from_layout(logical_size, sidebar_collapsed);
        let bounds = Self::content_bounds(logical_size, sidebar_collapsed);

        // Show target first (on top of Z-order), then hide others to eliminate any white flash
        if let Some(session) = self.sessions.get_mut(account_id) {
            if session.last_bounds != bounds_key {
                let _ = session.webview.set_bounds(bounds);
                session.last_bounds = bounds_key;
            }
            if session.last_zoom != zoom_percent {
                let _ = session.webview.zoom((zoom_percent as f64) / 100.0);
                session.last_zoom = zoom_percent;
            }
            let want_visible = !settings_open;
            if want_visible {
                set_webview_hwnd_visible(&session.webview, true);
                session.visible = true;
                let _ = session.webview.focus();
            } else if session.visible {
                set_webview_hwnd_visible(&session.webview, false);
                session.visible = false;
            }
        }

        for (id, session) in &mut self.sessions {
            if id != account_id && session.visible {
                set_webview_hwnd_visible(&session.webview, false);
                session.visible = false;
            }
        }

        Ok(())
    }

    pub fn hide_all(&mut self) {
        for session in self.sessions.values_mut() {
            if session.visible {
                set_webview_hwnd_visible(&session.webview, false);
                session.visible = false;
            }
        }
    }

    pub fn show_active(
        &mut self,
        active_id: Option<&str>,
        logical_size: LogicalSize<f64>,
        sidebar_collapsed: bool,
    ) {
        let bounds_key = BoundsKey::from_layout(logical_size, sidebar_collapsed);
        let bounds = Self::content_bounds(logical_size, sidebar_collapsed);
        for (id, session) in &mut self.sessions {
            let is_active = active_id == Some(id.as_str());
            if is_active {
                if session.last_bounds != bounds_key {
                    let _ = session.webview.set_bounds(bounds);
                    session.last_bounds = bounds_key;
                }
                set_webview_hwnd_visible(&session.webview, true);
                session.visible = true;
                let _ = session.webview.focus();
            } else if session.visible {
                set_webview_hwnd_visible(&session.webview, false);
                session.visible = false;
            }
        }
    }

    pub fn resize_all(&mut self, logical_size: LogicalSize<f64>, sidebar_collapsed: bool) {
        let bounds_key = BoundsKey::from_layout(logical_size, sidebar_collapsed);
        let bounds = Self::content_bounds(logical_size, sidebar_collapsed);
        for session in self.sessions.values_mut() {
            if session.last_bounds != bounds_key {
                let _ = session.webview.set_bounds(bounds);
                session.last_bounds = bounds_key;
            }
        }
    }

    pub fn apply_zoom_all(&mut self, zoom_percent: u32) {
        let factor = (zoom_percent as f64) / 100.0;
        for session in self.sessions.values_mut() {
            if session.last_zoom != zoom_percent {
                let _ = session.webview.zoom(factor);
                session.last_zoom = zoom_percent;
            }
        }
    }

    pub fn reload_account(&self, account_id: &str) {
        if let Some(session) = self.sessions.get(account_id) {
            let _ = session.webview.evaluate_script("window.location.reload();");
        }
    }

    pub fn drop_and_delete_profile(&mut self, account_id: &str) {
        self.sessions.remove(account_id);
        let dir = AppConfig::profile_dir(account_id);
        thread::spawn(move || {
            for _ in 0..10 {
                if !dir.exists() {
                    break;
                }
                if fs::remove_dir_all(&dir).is_ok() {
                    break;
                }
                thread::sleep(Duration::from_millis(250));
            }
        });
    }
}

fn unique_download_path(dir: &PathBuf, filename: &PathBuf) -> PathBuf {
    let candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }
    let stem = filename
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let ext = filename
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();

    for i in 1..1000 {
        let next = dir.join(format!("{stem} ({i}){ext}"));
        if !next.exists() {
            return next;
        }
    }
    candidate
}
