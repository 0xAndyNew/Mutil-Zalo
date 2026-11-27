use std::fs;
use std::path::PathBuf;

use tao::event_loop::EventLoopProxy;

use crate::config::AppConfig;
use crate::icon;
use crate::ipc::UserEvent;

pub const APP_USER_MODEL_ID: &str = "MutilZalo.Desktop";

#[cfg(windows)]
use windows::core::{HSTRING, IInspectable};
#[cfg(windows)]
use windows::Data::Xml::Dom::XmlDocument;
#[cfg(windows)]
use windows::Foundation::TypedEventHandler;
#[cfg(windows)]
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

#[cfg(windows)]
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE,
    REG_OPTION_NON_VOLATILE, REG_SZ,
};
#[cfg(windows)]
use windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

use crate::win_utils::to_wide_null;

fn icon_cache_path() -> PathBuf {
    AppConfig::app_data_root().join("mutil-zalo.ico")
}

/// Registers an HKCU AppUserModelId and writes our `.ico` so Windows 10/11 displays native
/// Toast Notifications with the Mutil-Zalo app name & icon (no Admin rights required).
pub fn init_notifications() {
    let ico_path = icon_cache_path();
    if let Some(parent) = ico_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let ico_bytes = icon::generate_ico_bytes();
    let needs_write = fs::metadata(&ico_path)
        .map(|m| m.len() != ico_bytes.len() as u64)
        .unwrap_or(true);
    if needs_write {
        let _ = fs::write(&ico_path, &ico_bytes);
    }

    #[cfg(windows)]
    unsafe {
        let aumid_w = to_wide_null(APP_USER_MODEL_ID);
        let _ = SetCurrentProcessExplicitAppUserModelID(aumid_w.as_ptr());

        let subkey = to_wide_null(&format!(
            "Software\\Classes\\AppUserModelId\\{APP_USER_MODEL_ID}"
        ));
        let mut hkey: HKEY = std::ptr::null_mut();
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            std::ptr::null(),
            &mut hkey,
            std::ptr::null_mut(),
        ) == 0
        {
            let pairs = [
                ("DisplayName", "Mutil-Zalo".to_string()),
                ("IconUri", ico_path.to_string_lossy().to_string()),
                ("IconBackgroundColor", "FF0068FF".to_string()),
            ];
            for (k, v) in pairs {
                let kw = to_wide_null(k);
                let vw = to_wide_null(&v);
                RegSetValueExW(
                    hkey,
                    kw.as_ptr(),
                    0,
                    REG_SZ,
                    vw.as_ptr() as *const u8,
                    (vw.len() * 2) as u32,
                );
            }
            RegCloseKey(hkey);
        }
    }
}

pub fn xml_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Strip illegal XML control characters
            c if (c as u32) < 0x20 && c != '\n' && c != '\r' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

pub fn build_toast_xml(account_name: &str, title: &str, body: &str) -> String {
    let header = if title.trim().is_empty() {
        account_name.to_string()
    } else if title.contains(account_name) {
        title.trim().to_string()
    } else {
        format!("{} • {}", account_name, title.trim())
    };
    let clean_body = if body.trim().is_empty() {
        "Bạn có tin nhắn mới trên Zalo".to_string()
    } else {
        body.trim().to_string()
    };

    format!(
        r#"<toast scenario="default">
  <visual>
    <binding template="ToastGeneric">
      <text>{}</text>
      <text>{}</text>
    </binding>
  </visual>
  <audio src="ms-winsoundevent:Notification.IM" />
</toast>"#,
        xml_escape(&header),
        xml_escape(&clean_body)
    )
}

/// Shows a native Windows 10/11 Toast Notification. Clicking the toast restores Mutil-Zalo and switches
/// directly to `account_id` (if `Some`).
pub fn show_windows_toast(
    account_id: Option<String>,
    account_name: &str,
    title: &str,
    body: &str,
    proxy: EventLoopProxy<UserEvent>,
) {
    #[cfg(windows)]
    {
        let xml_str = build_toast_xml(account_name, title, body);
        let Ok(doc) = XmlDocument::new() else {
            return;
        };
        if doc.LoadXml(&HSTRING::from(xml_str)).is_err() {
            return;
        }
        let Ok(toast) = ToastNotification::CreateToastNotification(&doc) else {
            return;
        };

        let handler = TypedEventHandler::<ToastNotification, IInspectable>::new(
            move |_sender, _args| {
                if let Some(ref id) = account_id {
                    let _ = proxy.send_event(UserEvent::TraySwitchAccount(id.clone()));
                } else {
                    let _ = proxy.send_event(UserEvent::TrayShow);
                }
                Ok(())
            },
        );
        let _ = toast.Activated(&handler);

        if let Ok(notifier) =
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_USER_MODEL_ID))
        {
            let _ = notifier.Show(&toast);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (account_id, account_name, title, body, proxy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_xml_and_formats_toast() {
        let xml = build_toast_xml("Zalo Công việc", "A & B <test>", "Tin nhắn \"mới\"");
        assert!(xml.contains("Zalo Công việc • A &amp; B &lt;test&gt;"));
        assert!(xml.contains("Tin nhắn &quot;mới&quot;"));
        assert!(xml.contains("ms-winsoundevent:Notification.IM"));
    }
}
