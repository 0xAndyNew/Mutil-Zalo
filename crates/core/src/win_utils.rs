use std::ffi::OsStr;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::icon;

pub const SINGLE_INSTANCE_MUTEX: &str = "Local\\MutilZaloSingleInstanceMutex";

#[cfg(windows)]
use windows::core::{Interface, HSTRING};
#[cfg(windows)]
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile,
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
#[cfg(windows)]
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileOpenDialog, IShellItem, IShellLinkW, SHCreateItemFromParsingName,
    ShellLink, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};

#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM};
#[cfg(windows)]
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, KEY_WRITE, REG_DWORD,
    REG_OPTION_NON_VOLATILE, REG_SZ,
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

pub fn default_install_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Programs")
        .join("Mutil-Zalo")
}

#[cfg(windows)]
fn registered_install_dir() -> Option<PathBuf> {
    unsafe {
        let subkey =
            to_wide_null("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Mutil-Zalo");
        let value = to_wide_null("InstallLocation");
        let mut hkey: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) != 0 {
            return None;
        }
        let mut byte_len: u32 = 0;
        if RegQueryValueExW(
            hkey,
            value.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut byte_len,
        ) != 0
            || byte_len < 4
        {
            RegCloseKey(hkey);
            return None;
        }
        let mut buf = vec![0u16; (byte_len as usize + 1) / 2];
        let ok = RegQueryValueExW(
            hkey,
            value.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            buf.as_mut_ptr() as *mut u8,
            &mut byte_len,
        ) == 0;
        RegCloseKey(hkey);
        if !ok {
            return None;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        let s = String::from_utf16_lossy(&buf[..end]).trim().to_string();
        if s.is_empty() {
            None
        } else {
            Some(PathBuf::from(s))
        }
    }
}

#[cfg(not(windows))]
fn registered_install_dir() -> Option<PathBuf> {
    None
}

pub fn install_dir() -> PathBuf {
    registered_install_dir().unwrap_or_else(default_install_dir)
}

pub fn pick_install_folder(owner: &tao::window::Window, current_dir: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    unsafe {
        use tao::platform::windows::WindowExtWindows;
        let owner_hwnd = windows::Win32::Foundation::HWND(owner.hwnd() as *mut _);
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let should_uninit = hr.is_ok();

        let picked = (|| -> Option<PathBuf> {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
            let opts = dialog.GetOptions().ok()?;
            dialog
                .SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)
                .ok()?;
            dialog
                .SetTitle(&HSTRING::from("Chọn thư mục cài đặt Mutil-Zalo"))
                .ok()?;

            let mut probe = current_dir.to_path_buf();
            while !probe.exists() {
                if !probe.pop() {
                    break;
                }
            }
            if probe.exists() {
                if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(
                    &HSTRING::from(probe.as_os_str()),
                    None,
                ) {
                    let _ = dialog.SetFolder(&item);
                }
            }

            dialog.Show(Some(owner_hwnd)).ok()?;
            let item = dialog.GetResult().ok()?;
            let pwstr = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
            let path_str = pwstr.to_string().ok();
            CoTaskMemFree(Some(pwstr.0 as *const _));
            path_str.map(|s| {
                let p = PathBuf::from(s);
                let named = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.eq_ignore_ascii_case("Mutil-Zalo"))
                    .unwrap_or(false);
                if named { p } else { p.join("Mutil-Zalo") }
            })
        })();

        if should_uninit {
            CoUninitialize();
        }
        picked
    }
    #[cfg(not(windows))]
    {
        let _ = (owner, current_dir);
        None
    }
}

pub fn installed_exe_path() -> PathBuf {
    install_dir().join("Mutil-Zalo.exe")
}

pub fn is_installed() -> bool {
    installed_exe_path().exists()
}

pub fn desktop_shortcut_path() -> Option<PathBuf> {
    dirs::desktop_dir().map(|d| d.join("Mutil-Zalo.lnk"))
}

pub fn start_menu_shortcut_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("Microsoft\\Windows\\Start Menu\\Programs\\Mutil-Zalo.lnk"))
}

pub fn has_desktop_shortcut() -> bool {
    desktop_shortcut_path().map(|p| p.exists()).unwrap_or(false)
}

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub install_dir: Option<PathBuf>,
    pub create_desktop_shortcut: bool,
    pub create_start_menu_shortcut: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            install_dir: None,
            create_desktop_shortcut: true,
            create_start_menu_shortcut: true,
        }
    }
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

/// Installs the single `Mutil-Zalo.exe` into %LOCALAPPDATA%\Programs\Mutil-Zalo,
/// creates optional Desktop / Start Menu shortcuts natively via COM, and registers Windows Uninstall entry.
pub fn install_to_windows(opts: InstallOptions) -> Result<String, String> {
    let dir = opts
        .install_dir
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(install_dir);
    fs::create_dir_all(&dir).map_err(|e| format!("Không thể tạo thư mục cài đặt: {e}"))?;

    let src_exe = launcher_exe_path();
    let dst_exe = dir.join("Mutil-Zalo.exe");
    if src_exe.canonicalize().ok() != dst_exe.canonicalize().ok() {
        fs::copy(&src_exe, &dst_exe)
            .map_err(|e| format!("Không thể sao chép file ứng dụng: {e}"))?;
    }

    let ico_path = dir.join("mutil-zalo.ico");
    fs::write(&ico_path, icon::generate_ico_bytes())
        .map_err(|e| format!("Không thể ghi icon: {e}"))?;

    let workdir = dst_exe.parent().unwrap_or_else(|| Path::new("."));
    if opts.create_desktop_shortcut {
        if let Some(desktop) = desktop_shortcut_path() {
            create_lnk_shortcut(
                &desktop,
                &dst_exe,
                workdir,
                &ico_path,
                "Mutil-Zalo - Zalo Web Đa Tài Khoản",
            )?;
        }
    }
    if opts.create_start_menu_shortcut {
        if let Some(start_menu) = start_menu_shortcut_path() {
            create_lnk_shortcut(
                &start_menu,
                &dst_exe,
                workdir,
                &ico_path,
                "Mutil-Zalo - Zalo Web Đa Tài Khoản",
            )?;
        }
    }

    register_uninstall_entry(&dir, &dst_exe, &ico_path)?;

    let mut extras = Vec::new();
    if opts.create_desktop_shortcut {
        extras.push("Shortcut Desktop");
    }
    if opts.create_start_menu_shortcut {
        extras.push("Start Menu");
    }
    if extras.is_empty() {
        Ok(format!("Đã cài đặt Mutil-Zalo thành công tại {}", dir.display()))
    } else {
        Ok(format!(
            "Đã cài đặt Mutil-Zalo thành công (đã tạo {}).",
            extras.join(" & ")
        ))
    }
}

/// Creates or refreshes the Desktop shortcut for Mutil-Zalo immediately.
pub fn create_desktop_shortcut_now() -> Result<String, String> {
    let desktop = desktop_shortcut_path()
        .ok_or_else(|| "Không tìm thấy thư mục Desktop trên Windows.".to_string())?;
    let target_exe = if is_installed() {
        installed_exe_path()
    } else {
        launcher_exe_path()
    };
    let workdir = target_exe.parent().unwrap_or_else(|| Path::new("."));

    let ico_path = if is_installed() {
        install_dir().join("mutil-zalo.ico")
    } else {
        crate::config::AppConfig::app_data_root().join("mutil-zalo.ico")
    };
    if let Some(parent) = ico_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(&ico_path, icon::generate_ico_bytes())
        .map_err(|e| format!("Không thể ghi file icon: {e}"))?;

    create_lnk_shortcut(
        &desktop,
        &target_exe,
        workdir,
        &ico_path,
        "Mutil-Zalo - Zalo Web Đa Tài Khoản",
    )?;
    Ok("Đã tạo biểu tượng (Shortcut) Mutil-Zalo ngoài Desktop.".to_string())
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

#[cfg(windows)]
pub fn create_lnk_shortcut(
    link_path: &Path,
    target_exe: &Path,
    working_dir: &Path,
    ico_path: &Path,
    description: &str,
) -> Result<(), String> {
    if let Some(parent) = link_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Không thể tạo thư mục shortcut: {e}"))?;
    }

    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let should_uninit = hr.is_ok();

        let res = (|| -> Result<(), String> {
            let shell_link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("Không thể khởi tạo IShellLinkW: {e}"))?;
            shell_link
                .SetPath(&HSTRING::from(target_exe.as_os_str()))
                .map_err(|e| format!("SetPath thất bại: {e}"))?;
            shell_link
                .SetWorkingDirectory(&HSTRING::from(working_dir.as_os_str()))
                .map_err(|e| format!("SetWorkingDirectory thất bại: {e}"))?;
            shell_link
                .SetDescription(&HSTRING::from(description))
                .map_err(|e| format!("SetDescription thất bại: {e}"))?;
            shell_link
                .SetIconLocation(&HSTRING::from(ico_path.as_os_str()), 0)
                .map_err(|e| format!("SetIconLocation thất bại: {e}"))?;

            let persist: IPersistFile = shell_link
                .cast()
                .map_err(|e| format!("Không thể lấy IPersistFile: {e}"))?;
            persist
                .Save(&HSTRING::from(link_path.as_os_str()), true)
                .map_err(|e| format!("Lưu shortcut thất bại: {e}"))?;
            Ok(())
        })();

        if should_uninit {
            CoUninitialize();
        }
        res
    }
}

#[cfg(not(windows))]
pub fn create_lnk_shortcut(
    _link_path: &Path,
    _target_exe: &Path,
    _working_dir: &Path,
    _ico_path: &Path,
    _description: &str,
) -> Result<(), String> {
    Ok(())
}

fn remove_shortcuts() {
    if let Some(p) = desktop_shortcut_path() {
        let _ = fs::remove_file(p);
    }
    if let Some(p) = start_menu_shortcut_path() {
        let _ = fs::remove_file(p);
    }
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

    #[cfg(windows)]
    #[test]
    fn creates_real_windows_lnk_shortcut_via_com() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let tmp = std::env::temp_dir().join(format!("mutil-zalo-core-lnk-{ts}"));
        let link = tmp.join("Mutil-Zalo.lnk");
        let target = std::env::current_exe().unwrap();
        let workdir = target.parent().unwrap();

        create_lnk_shortcut(
            &link,
            &target,
            workdir,
            &target,
            "Mutil-Zalo - Zalo Web Đa Tài Khoản",
        )
        .expect("create_lnk_shortcut should succeed");

        let bytes = fs::read(&link).expect("shortcut file should exist");
        assert!(bytes.len() > 76);
        assert_eq!(&bytes[0..4], &[0x4C, 0x00, 0x00, 0x00]);
        let _ = fs::remove_dir_all(&tmp);
    }
}
