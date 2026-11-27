#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE,
    KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::CreateMutexW;
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowTextW, IsIconic, IsWindow, MessageBoxW,
    SetForegroundWindow, ShowWindow, IDOK, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONQUESTION,
    MB_OK, MB_OKCANCEL, SW_RESTORE, SW_SHOW,
};

fn to_wide_null(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
fn msg_box(title: &str, text: &str, flags: u32) -> i32 {
    unsafe {
        let t = to_wide_null(title);
        let m = to_wide_null(text);
        MessageBoxW(std::ptr::null_mut(), m.as_ptr(), t.as_ptr(), flags)
    }
}

#[cfg(not(windows))]
fn msg_box(_title: &str, _text: &str, _flags: u32) -> i32 {
    0
}

fn local_app_data() -> PathBuf {
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn roaming_app_data() -> PathBuf {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(local_app_data)
}

fn desktop_dir() -> PathBuf {
    env::var_os("USERPROFILE")
        .map(|p| PathBuf::from(p).join("Desktop"))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn runtime_root() -> PathBuf {
    local_app_data().join("Mutil-Zalo").join("runtime")
}

fn install_dir() -> PathBuf {
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

fn ensure_webview2() -> bool {
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

fn unpack_runtime_if_needed() -> Result<PathBuf, String> {
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

fn run_cli_install() {
    let dir = install_dir();
    let _ = fs::create_dir_all(&dir);
    let dst_exe = dir.join("Mutil-Zalo.exe");
    if let Ok(src) = env::current_exe() {
        if src.canonicalize().ok() != dst_exe.canonicalize().ok() {
            if let Err(e) = fs::copy(&src, &dst_exe) {
                msg_box(
                    "Mutil-Zalo — Lỗi cài đặt",
                    &format!("Không thể sao chép file cài đặt: {e}"),
                    MB_OK | MB_ICONERROR,
                );
                return;
            }
        }
    }

    let desktop = desktop_dir().join("Mutil-Zalo.lnk");
    let start_menu =
        roaming_app_data().join("Microsoft\\Windows\\Start Menu\\Programs\\Mutil-Zalo.lnk");
    let target = dst_exe.to_string_lossy().replace('\'', "''");
    let workdir = dir.to_string_lossy().replace('\'', "''");
    let mut script = String::from("$ws = New-Object -ComObject WScript.Shell\n");
    for link in [desktop, start_menu] {
        let lp = link.to_string_lossy().replace('\'', "''");
        script.push_str(&format!(
            "$s = $ws.CreateShortcut('{lp}'); $s.TargetPath = '{target}'; $s.WorkingDirectory = '{workdir}'; $s.Description = 'Mutil-Zalo - Zalo Web Đa Tài Khoản'; $s.Save()\n"
        ));
    }
    let _ = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .status();

    #[cfg(windows)]
    unsafe {
        let subkey =
            to_wide_null("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Mutil-Zalo");
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
                ("DisplayName", "Mutil-Zalo (Zalo Web Đa Tài Khoản)".to_string()),
                ("DisplayVersion", env!("CARGO_PKG_VERSION").to_string()),
                ("Publisher", "Mutil-Zalo".to_string()),
                ("InstallLocation", dir.to_string_lossy().to_string()),
                (
                    "UninstallString",
                    format!("\"{}\" --uninstall", dst_exe.to_string_lossy()),
                ),
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
            for k in ["NoModify", "NoRepair"] {
                let kw = to_wide_null(k);
                let one: u32 = 1;
                RegSetValueExW(hkey, kw.as_ptr(), 0, REG_DWORD, &one as *const u32 as *const u8, 4);
            }
            RegCloseKey(hkey);
        }
    }

    msg_box(
        "Mutil-Zalo",
        &format!(
            "Đã cài đặt Mutil-Zalo thành công!\n\nThư mục: {}\nĐã tạo shortcut trên Desktop và Start Menu.",
            dir.display()
        ),
        MB_OK | MB_ICONINFORMATION,
    );
}

fn run_cli_uninstall() {
    let _ = fs::remove_file(desktop_dir().join("Mutil-Zalo.lnk"));
    let _ = fs::remove_file(
        roaming_app_data().join("Microsoft\\Windows\\Start Menu\\Programs\\Mutil-Zalo.lnk"),
    );

    #[cfg(windows)]
    unsafe {
        let run_key = to_wide_null("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let val = to_wide_null("Mutil-Zalo");
        let mut hkey: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, run_key.as_ptr(), 0, KEY_SET_VALUE, &mut hkey) == 0 {
            let _ = RegDeleteValueW(hkey, val.as_ptr());
            RegCloseKey(hkey);
        }
        let uninst =
            to_wide_null("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Mutil-Zalo");
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, uninst.as_ptr());
    }

    let dir = install_dir();
    let _ = fs::remove_dir_all(runtime_root());

    // Schedule removal of install dir after process exits
    let cmd = format!(
        "ping 127.0.0.1 -n 2 > nul & rmdir /s /q \"{}\"",
        dir.display()
    );
    let _ = Command::new("cmd.exe")
        .args(["/C", &cmd])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();

    msg_box(
        "Mutil-Zalo",
        "Đã gỡ cài đặt Mutil-Zalo khỏi Windows.\n(Dữ liệu phiên Zalo trong AppData được giữ an toàn nếu bạn muốn dùng lại.)",
        MB_OK | MB_ICONINFORMATION,
    );
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == "--install") {
        run_cli_install();
        return;
    }
    if args.iter().any(|a| a == "--uninstall") {
        run_cli_uninstall();
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
