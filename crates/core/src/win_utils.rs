use std::ffi::OsStr;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::icon;

pub const SINGLE_INSTANCE_MUTEX: &str = "Local\\MutilZaloSingleInstanceMutex";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM};
#[cfg(windows)]
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::CreateMutexW;
#[cfg(windows)]
use windows_sys::Win32::UI::Shell::ShellExecuteW;
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FlashWindowEx, GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW,
    IsIconic, IsWindow, SetForegroundWindow, ShowWindow, FLASHWINFO, FLASHW_TIMERNOFG,
    FLASHW_TRAY, SW_RESTORE, SW_SHOW,
};

pub fn to_wide_null(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub struct SingleInstanceGuard {
    #[cfg(windows)]
    handle: HANDLE,
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            if !self.handle.is_null() {
                CloseHandle(self.handle);
            }
        }
    }
}

/// Acquires the Windows single-instance mutex. Returns None if another instance is already running.
pub fn acquire_single_instance() -> Option<SingleInstanceGuard> {
    #[cfg(windows)]
    unsafe {
        let name = to_wide_null(SINGLE_INSTANCE_MUTEX);
        let handle = CreateMutexW(std::ptr::null(), 1, name.as_ptr());
        if handle.is_null() {
            return Some(SingleInstanceGuard { handle });
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(handle);
            focus_existing_window();
            return None;
        }
        Some(SingleInstanceGuard { handle })
    }
    #[cfg(not(windows))]
    {
        Some(SingleInstanceGuard {})
    }
}

pub fn hwnd_file_path() -> PathBuf {
    crate::config::AppConfig::app_data_root().join("window.hwnd")
}

/// Saves the exact native Win32 HWND so secondary launches restore this exact window even when hidden in tray.
pub fn save_main_window_hwnd(window: &tao::window::Window) {
    #[cfg(windows)]
    {
        use tao::platform::windows::WindowExtWindows;
        let hwnd = window.hwnd() as isize;
        let path = hwnd_file_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(path, hwnd.to_string());
    }
    #[cfg(not(windows))]
    {
        let _ = window;
    }
}

#[cfg(windows)]
pub fn focus_existing_window() {
    if let Ok(raw) = fs::read_to_string(hwnd_file_path()) {
        if let Ok(val) = raw.trim().parse::<isize>() {
            let hwnd = val as HWND;
            unsafe {
                if !hwnd.is_null() && IsWindow(hwnd) != 0 {
                    ShowWindow(hwnd, SW_SHOW);
                    if IsIconic(hwnd) != 0 {
                        ShowWindow(hwnd, SW_RESTORE);
                    }
                    SetForegroundWindow(hwnd);
                    return;
                }
            }
        }
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, _lparam: LPARAM) -> i32 {
        let len = GetWindowTextLengthW(hwnd);
        if len > 0 {
            let mut buf = vec![0u16; (len + 1) as usize];
            let copied = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            if copied > 0 {
                let title = String::from_utf16_lossy(&buf[..copied as usize]);
                if title == "Mutil-Zalo" || title.contains("Mutil-Zalo — ") {
                    ShowWindow(hwnd, SW_SHOW);
                    if IsIconic(hwnd) != 0 {
                        ShowWindow(hwnd, SW_RESTORE);
                    }
                    SetForegroundWindow(hwnd);
                    return 0; // stop enumerating
                }
            }
        }
        1
    }

    unsafe {
        EnumWindows(Some(enum_proc), 0);
    }
}

pub fn open_external(target: &str) {
    #[cfg(windows)]
    unsafe {
        let op = to_wide_null("open");
        let file = to_wide_null(target);
        ShellExecuteW(
            std::ptr::null_mut(),
            op.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOW,
        );
    }
    #[cfg(not(windows))]
    {
        let _ = target;
    }
}

/// Path to the user-facing single `Mutil-Zalo.exe` (passed by launcher via env var).
pub fn launcher_exe_path() -> PathBuf {
    if let Ok(p) = std::env::var("MUTIL_ZALO_LAUNCHER_EXE") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return pb;
        }
    }
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("Mutil-Zalo.exe"))
}

pub fn install_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Programs")
        .join("Mutil-Zalo")
}

pub fn installed_exe_path() -> PathBuf {
    install_dir().join("Mutil-Zalo.exe")
}

pub fn is_installed() -> bool {
    installed_exe_path().exists()
}

/// Registers or removes Mutil-Zalo from Windows startup (HKCU\Software\Microsoft\Windows\CurrentVersion\Run).
pub fn set_start_with_windows(enable: bool) -> Result<(), String> {
    #[cfg(windows)]
    unsafe {
        let subkey = to_wide_null("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let value_name = to_wide_null("Mutil-Zalo");
        let mut hkey: HKEY = std::ptr::null_mut();
        let status = RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_SET_VALUE, &mut hkey);
        if status != 0 {
            return Err(format!("Không thể mở khóa Registry Run (mã lỗi {status})"));
        }

        let res = if enable {
            let exe = if is_installed() {
                installed_exe_path()
            } else {
                launcher_exe_path()
            };
            let quoted = format!("\"{}\"", exe.display());
            let data = to_wide_null(&quoted);
            let s = RegSetValueExW(
                hkey,
                value_name.as_ptr(),
                0,
                REG_SZ,
                data.as_ptr() as *const u8,
                (data.len() * 2) as u32,
            );
            if s == 0 {
                Ok(())
            } else {
                Err(format!("Không thể ghi Registry Run (mã lỗi {s})"))
            }
        } else {
            let _ = RegDeleteValueW(hkey, value_name.as_ptr());
            Ok(())
        };

        RegCloseKey(hkey);
        res
    }
    #[cfg(not(windows))]
    {
        let _ = enable;
        Ok(())
    }
}

/// Installs the single `Mutil-Zalo.exe` into %LOCALAPPDATA%\Programs\Mutil-Zalo, creates Desktop + Start Menu shortcuts and Windows Uninstall entry.
pub fn install_to_windows() -> Result<String, String> {
    let dir = install_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Không thể tạo thư mục cài đặt: {e}"))?;

    let src_exe = launcher_exe_path();
    let dst_exe = installed_exe_path();
    if src_exe.canonicalize().ok() != dst_exe.canonicalize().ok() {
        fs::copy(&src_exe, &dst_exe)
            .map_err(|e| format!("Không thể sao chép file ứng dụng: {e}"))?;
    }

    let ico_path = dir.join("mutil-zalo.ico");
    fs::write(&ico_path, icon::generate_ico_bytes())
        .map_err(|e| format!("Không thể ghi icon: {e}"))?;

    create_shortcuts(&dst_exe, &ico_path)?;
    register_uninstall_entry(&dir, &dst_exe, &ico_path)?;

    Ok(format!("Đã cài đặt Mutil-Zalo thành công tại {}", dir.display()))
}

pub fn uninstall_from_windows() -> Result<String, String> {
    let _ = set_start_with_windows(false);
    remove_shortcuts();
    remove_uninstall_entry();

    let dir = install_dir();
    let exe = installed_exe_path();
    let ico = dir.join("mutil-zalo.ico");
    let _ = fs::remove_file(&ico);
    let _ = fs::remove_file(&exe);
    let _ = fs::remove_dir(&dir);

    Ok("Đã gỡ cài đặt shortcut và mục ứng dụng khỏi Windows.".to_string())
}

fn create_shortcuts(exe_path: &Path, ico_path: &Path) -> Result<(), String> {
    let desktop = dirs::desktop_dir().map(|d| d.join("Mutil-Zalo.lnk"));
    let start_menu = dirs::data_dir()
        .map(|d| d.join("Microsoft\\Windows\\Start Menu\\Programs\\Mutil-Zalo.lnk"));

    let mut script = String::from("$ws = New-Object -ComObject WScript.Shell\n");
    let target = ps_escape(&exe_path.to_string_lossy());
    let workdir = ps_escape(
        &exe_path
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    );
    let ico = ps_escape(&ico_path.to_string_lossy());

    for link in [desktop, start_menu].into_iter().flatten() {
        let lp = ps_escape(&link.to_string_lossy());
        script.push_str(&format!(
            "$s = $ws.CreateShortcut('{lp}'); $s.TargetPath = '{target}'; $s.WorkingDirectory = '{workdir}'; $s.IconLocation = '{ico},0'; $s.Description = 'Mutil-Zalo - Zalo Web Đa Tài Khoản'; $s.Save()\n"
        ));
    }

    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Không thể chạy PowerShell tạo shortcut: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "Tạo shortcut thất bại: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn remove_shortcuts() {
    if let Some(d) = dirs::desktop_dir() {
        let _ = fs::remove_file(d.join("Mutil-Zalo.lnk"));
    }
    if let Some(d) = dirs::data_dir() {
        let _ = fs::remove_file(d.join("Microsoft\\Windows\\Start Menu\\Programs\\Mutil-Zalo.lnk"));
    }
}

fn ps_escape(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(windows)]
fn register_uninstall_entry(dir: &Path, exe: &Path, ico: &Path) -> Result<(), String> {
    unsafe {
        let subkey =
            to_wide_null("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Mutil-Zalo");
        let mut hkey: HKEY = std::ptr::null_mut();
        let status = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            std::ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            std::ptr::null(),
            &mut hkey,
            std::ptr::null_mut(),
        );
        if status != 0 {
            return Err(format!("Không thể tạo khóa Uninstall (mã {status})"));
        }

        let entries = [
            ("DisplayName", "Mutil-Zalo (Zalo Web Đa Tài Khoản)".to_string()),
            ("DisplayVersion", env!("CARGO_PKG_VERSION").to_string()),
            ("Publisher", "Mutil-Zalo".to_string()),
            ("InstallLocation", dir.to_string_lossy().to_string()),
            ("DisplayIcon", format!("{},0", ico.to_string_lossy())),
            (
                "UninstallString",
                format!("\"{}\" --uninstall", exe.to_string_lossy()),
            ),
        ];

        for (k, v) in entries {
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

        for k in ["NoModify", "NoRepair"] {
            let kw = to_wide_null(k);
            let one: u32 = 1;
            RegSetValueExW(
                hkey,
                kw.as_ptr(),
                0,
                REG_DWORD,
                &one as *const u32 as *const u8,
                4,
            );
        }

        RegCloseKey(hkey);
        Ok(())
    }
}

#[cfg(not(windows))]
fn register_uninstall_entry(_dir: &Path, _exe: &Path, _ico: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn remove_uninstall_entry() {
    unsafe {
        let subkey =
            to_wide_null("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Mutil-Zalo");
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, subkey.as_ptr());
    }
}

#[cfg(not(windows))]
fn remove_uninstall_entry() {}

/// Determines whether a popup/new-window URL belongs to Zalo itself (auth, calls, internal viewer)
/// or is an external website that should open in the user's default browser.
pub fn is_zalo_internal_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    if lower.is_empty() || lower == "about:blank" || lower.starts_with("blob:") {
        return true;
    }
    let after_scheme = match lower.split_once("://") {
        Some((_, rest)) => rest,
        None => return false,
    };
    let host = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");

    matches_domain(host, "zalo.me")
        || matches_domain(host, "zaloapp.com")
        || matches_domain(host, "zdn.vn")
}

fn matches_domain(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

/// Flashes the Windows taskbar icon if Mutil-Zalo is not currently focused.
pub fn flash_taskbar_if_unfocused() {
    #[cfg(windows)]
    unsafe {
        unsafe extern "system" fn find_and_flash(hwnd: HWND, _lparam: LPARAM) -> i32 {
            let len = GetWindowTextLengthW(hwnd);
            if len > 0 {
                let mut buf = vec![0u16; (len + 1) as usize];
                let copied = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
                if copied > 0 {
                    let title = String::from_utf16_lossy(&buf[..copied as usize]);
                    if title.contains("Mutil-Zalo") {
                        if GetForegroundWindow() != hwnd {
                            let info = FLASHWINFO {
                                cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
                                hwnd,
                                dwFlags: FLASHW_TRAY | FLASHW_TIMERNOFG,
                                uCount: 4,
                                dwTimeout: 0,
                            };
                            FlashWindowEx(&info);
                        }
                        return 0;
                    }
                }
            }
            1
        }
        EnumWindows(Some(find_and_flash), 0);
    }
}

/// Parses unread message count from Zalo Web's document title, e.g. "(5) Zalo" -> 5, "(99+) Zalo" -> 99.
#[cfg(test)]
pub fn parse_unread_from_title(title: &str) -> u32 {
    let trimmed = title.trim();
    if !trimmed.starts_with('(') {
        return 0;
    }
    let Some(end) = trimmed.find(')') else {
        return 0;
    };
    let inner = trimmed[1..end].trim().trim_end_matches('+');
    inner.parse::<u32>().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_zalo_unread_titles() {
        assert_eq!(parse_unread_from_title("Zalo"), 0);
        assert_eq!(parse_unread_from_title("(1) Zalo"), 1);
        assert_eq!(parse_unread_from_title("(12) Zalo - Tin nhắn mới"), 12);
        assert_eq!(parse_unread_from_title("(99+) Zalo"), 99);
        assert_eq!(parse_unread_from_title("(abc) Zalo"), 0);
        assert_eq!(parse_unread_from_title(""), 0);
    }

    #[test]
    fn classifies_internal_vs_external_urls() {
        assert!(is_zalo_internal_url("https://chat.zalo.me/"));
        assert!(is_zalo_internal_url("https://id.zalo.me/account"));
        assert!(is_zalo_internal_url("https://call.zaloapp.com/room/123"));
        assert!(is_zalo_internal_url("about:blank"));
        assert!(!is_zalo_internal_url("https://www.youtube.com/watch?v=1"));
        assert!(!is_zalo_internal_url("https://fakezalo.me/phish"));
        assert!(!is_zalo_internal_url("https://google.com"));
    }
}
