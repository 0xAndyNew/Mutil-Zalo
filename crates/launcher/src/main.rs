#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod installer;

use flate2::read::GzDecoder;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io::Read;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

include!(concat!(env!("OUT_DIR"), "/payload_meta.rs"));

const CORE_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/core.exe.gz"));
const DLL_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/webview2loader.dll.gz"));

const SINGLE_INSTANCE_MUTEX: &str = "Local\\MutilZaloSingleInstanceMutex";
const WEBVIEW2_BOOTSTRAPPER_URL: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM};
#[cfg(windows)]
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE,
    KEY_READ,
};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::CreateMutexW;
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowTextW, IsIconic, IsWindow, MessageBoxW,
    SetForegroundWindow, ShowWindow, IDOK, MB_ICONERROR, MB_ICONQUESTION, MB_OK, MB_OKCANCEL,
    SW_RESTORE, SW_SHOW,
};

pub(crate) fn to_wide_null(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
pub(crate) fn msg_box(title: &str, text: &str, flags: u32) -> i32 {
    unsafe {
        let t = to_wide_null(title);
        let m = to_wide_null(text);
        MessageBoxW(std::ptr::null_mut(), m.as_ptr(), t.as_ptr(), flags)
    }
}

#[cfg(not(windows))]
pub(crate) fn msg_box(_title: &str, _text: &str, _flags: u32) -> i32 {
    0
}

fn local_app_data() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub(crate) fn runtime_root() -> PathBuf {
    local_app_data().join("Mutil-Zalo").join("runtime")
}

pub(crate) fn install_dir() -> PathBuf {
    local_app_data().join("Programs").join("Mutil-Zalo")
}

#[cfg(windows)]
fn is_already_running() -> bool {
    unsafe {
        let name = to_wide_null(SINGLE_INSTANCE_MUTEX);
        let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if handle.is_null() {
            return false;
        }
        let exists = GetLastError() == ERROR_ALREADY_EXISTS;
        CloseHandle(handle);
        exists
    }
}

#[cfg(not(windows))]
fn is_already_running() -> bool {
    false
}

#[cfg(windows)]
fn focus_existing_window() {
    let hwnd_file = local_app_data().join("Mutil-Zalo").join("window.hwnd");
    if let Ok(raw) = fs::read_to_string(&hwnd_file) {
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
                    return 0;
                }
            }
        }
        1
    }
    unsafe {
        EnumWindows(Some(enum_proc), 0);
    }
}

#[cfg(windows)]
fn is_webview2_installed() -> bool {
    const PATHS: &[(HKEY, &str)] = &[
        (
            HKEY_LOCAL_MACHINE,
            "SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        ),
        (
            HKEY_LOCAL_MACHINE,
            "SOFTWARE\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        ),
        (
            HKEY_CURRENT_USER,
            "Software\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        ),
    ];

    unsafe {
        let pv = to_wide_null("pv");
        for &(root, subkey) in PATHS {
            let sub = to_wide_null(subkey);
            let mut hkey: HKEY = std::ptr::null_mut();
            if RegOpenKeyExW(root, sub.as_ptr(), 0, KEY_READ, &mut hkey) == 0 {
                let mut len: u32 = 0;
                let ok = RegQueryValueExW(
                    hkey,
                    pv.as_ptr(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    &mut len,
                ) == 0
                    && len > 4;
                RegCloseKey(hkey);
                if ok {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(not(windows))]
fn is_webview2_installed() -> bool {
    true
}

pub(crate) fn ensure_webview2() -> bool {
    if is_webview2_installed() {
        return true;
    }

    let choice = msg_box(
        "Mutil-Zalo — Cần Microsoft Edge WebView2",
        "Máy tính chưa cài đặt Microsoft Edge WebView2 Runtime (thành phần cần thiết để chạy Zalo Web).\n\nBấm OK để tự động tải và cài đặt từ Microsoft (~2 MB).",
        MB_OKCANCEL | MB_ICONQUESTION,
    );
    if choice != IDOK {
        return false;
    }

    let bootstrapper = env::temp_dir().join("MicrosoftEdgeWebview2Setup.exe");
    let dl_cmd = format!(
        "[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12; Invoke-WebRequest -Uri '{}' -OutFile '{}'",
        WEBVIEW2_BOOTSTRAPPER_URL,
        bootstrapper.to_string_lossy().replace('\'', "''")
    );

    let dl_ok = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &dl_cmd])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !dl_ok || !bootstrapper.exists() {
        msg_box(
            "Mutil-Zalo — Lỗi tải WebView2",
            "Không thể tự tải Microsoft Edge WebView2 Runtime. Vui lòng kiểm tra kết nối mạng hoặc tải thủ công từ Microsoft.",
            MB_OK | MB_ICONERROR,
        );
        return false;
    }

    let _ = Command::new(&bootstrapper)
        .args(["/silent", "/install"])
        .status();
    let _ = fs::remove_file(&bootstrapper);

    if !is_webview2_installed() {
        msg_box(
            "Mutil-Zalo — Chưa hoàn tất cài WebView2",
            "Quá trình cài đặt WebView2 chưa hoàn tất. Vui lòng thử lại.",
            MB_OK | MB_ICONERROR,
        );
        return false;
    }
    true
}

fn gunzip_bytes(gz: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut decoder = GzDecoder::new(gz);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    Ok(out)
}

pub(crate) fn unpack_runtime_if_needed() -> Result<PathBuf, String> {
    let root = runtime_root();
    let version_dir = root.join(PAYLOAD_HASH);
    let core_path = version_dir.join("mutil-zalo-core.exe");
    let dll_path = version_dir.join("WebView2Loader.dll");

    let core_ready = fs::metadata(&core_path)
        .map(|m| m.len() == CORE_RAW_LEN)
        .unwrap_or(false);
    let dll_ready = fs::metadata(&dll_path)
        .map(|m| m.len() == DLL_RAW_LEN)
        .unwrap_or(false);

    if !core_ready || !dll_ready {
        fs::create_dir_all(&version_dir)
            .map_err(|e| format!("Không thể tạo thư mục runtime: {e}"))?;

        if !dll_ready {
            let dll_bytes =
                gunzip_bytes(DLL_GZ).map_err(|e| format!("Lỗi giải nén WebView2Loader.dll: {e}"))?;
            let tmp = dll_path.with_extension("dll.tmp");
            fs::write(&tmp, dll_bytes).map_err(|e| format!("Lỗi ghi WebView2Loader.dll: {e}"))?;
            let _ = fs::rename(&tmp, &dll_path);
        }

        if !core_ready {
            let core_bytes = gunzip_bytes(CORE_GZ)
                .map_err(|e| format!("Lỗi giải nén mutil-zalo-core.exe: {e}"))?;
            let tmp = core_path.with_extension("exe.tmp");
            fs::write(&tmp, core_bytes).map_err(|e| format!("Lỗi ghi mutil-zalo-core.exe: {e}"))?;
            let _ = fs::rename(&tmp, &core_path);
        }

        // Best-effort cleanup of older unpacked versions
        if let Ok(entries) = fs::read_dir(&root) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy() != PAYLOAD_HASH {
                    let _ = fs::remove_dir_all(entry.path());
                }
            }
        }
    }

    Ok(core_path)
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a.eq_ignore_ascii_case("--uninstall")) {
        installer::run_cli_uninstall();
        return;
    }

    let current_exe = env::current_exe().unwrap_or_else(|_| PathBuf::from("Mutil-Zalo.exe"));
    let installed_exe = installer::active_installed_exe();
    if installer::should_launch_installer(&current_exe, installed_exe.as_deref(), &args) {
        let silent = args
            .iter()
            .any(|a| a.eq_ignore_ascii_case("--silent") || a.eq_ignore_ascii_case("/S") || a == "-s");
        if silent {
            installer::run_silent_install(&args);
        } else {
            installer::run_gui_installer();
        }
        return;
    }

    if !HAS_REAL_PAYLOAD {
        msg_box(
            "Mutil-Zalo",
            "Bản build này chưa nhúng payload. Vui lòng build bằng script .\\build.ps1.",
            MB_OK | MB_ICONERROR,
        );
        return;
    }

    // Fast path: if already running, bring the existing Mutil-Zalo window to foreground immediately
    if is_already_running() {
        #[cfg(windows)]
        focus_existing_window();
        return;
    }

    if !ensure_webview2() {
        return;
    }

    let core_exe = match unpack_runtime_if_needed() {
        Ok(p) => p,
        Err(e) => {
            msg_box("Mutil-Zalo — Lỗi khởi chạy", &e, MB_OK | MB_ICONERROR);
            return;
        }
    };

    let launcher_path = env::current_exe().unwrap_or_else(|_| PathBuf::from("Mutil-Zalo.exe"));
    let runtime_dir = core_exe.parent().unwrap_or_else(|| Path::new("."));

    if let Err(e) = Command::new(&core_exe)
        .current_dir(runtime_dir)
        .env("MUTIL_ZALO_LAUNCHER_EXE", &launcher_path)
        .spawn()
    {
        msg_box(
            "Mutil-Zalo — Lỗi khởi chạy",
            &format!("Không thể mở ứng dụng Mutil-Zalo: {e}"),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_payload_decompresses_to_expected_sizes() {
        let core = gunzip_bytes(CORE_GZ).expect("core gunzip");
        let dll = gunzip_bytes(DLL_GZ).expect("dll gunzip");
        assert_eq!(core.len() as u64, CORE_RAW_LEN);
        assert_eq!(dll.len() as u64, DLL_RAW_LEN);
        assert_eq!(PAYLOAD_HASH.len(), 16);
    }
}
