use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{
    ensure_webview2, install_dir, runtime_root, to_wide_null, unpack_runtime_if_needed,
    HAS_REAL_PAYLOAD,
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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
    SHGetKnownFolderPath, ShellLink, FOLDERID_Desktop, FOLDERID_Programs, FOS_FORCEFILESYSTEM,
    FOS_PICKFOLDERS, KF_FLAG_DEFAULT, SIGDN_FILESYSPATH,
};

#[cfg(windows)]
use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
#[cfg(windows)]
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint,
    FillRect, GetDC, GetDeviceCaps, InvalidateRect, ReleaseDC, RoundRect, SelectObject, SetBkColor,
    SetBkMode, SetTextColor, UpdateWindow, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DEFAULT_PITCH, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX,
    DT_SINGLELINE, DT_VCENTER, FF_SWISS, FW_BOLD, FW_NORMAL, FW_SEMIBOLD, HBRUSH, HDC, HFONT,
    LOGPIXELSX, OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
#[cfg(windows)]
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(windows)]
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, KEY_WRITE, REG_DWORD,
    REG_OPTION_NON_VOLATILE, REG_SZ,
};
#[cfg(windows)]
use windows_sys::Win32::UI::Controls::{
    InitCommonControlsEx, DRAWITEMSTRUCT, ICC_PROGRESS_CLASS, ICC_STANDARD_CLASSES,
    INITCOMMONCONTROLSEX, PBM_SETPOS, PBM_SETRANGE,
};
#[cfg(windows)]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyWindow,
    DispatchMessageW, DrawIconEx, GetClientRect, GetMessageW, GetSystemMetrics, GetWindowLongPtrW,
    GetWindowTextLengthW, GetWindowTextW, IsDialogMessageW, LoadCursorW, LoadImageW, MessageBoxW,
    PostMessageW, PostQuitMessage, RegisterClassExW, SendMessageW, SetWindowLongPtrW,
    SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage, BS_AUTOCHECKBOX, BS_OWNERDRAW,
    CS_HREDRAW, CS_VREDRAW, DI_NORMAL, GWLP_USERDATA, HICON, IDC_ARROW, IMAGE_ICON,
    LR_DEFAULTCOLOR, MB_ICONERROR, MB_ICONWARNING, MB_OK, MSG, SM_CXSCREEN, SM_CYSCREEN, SWP_NOZORDER,
    SW_HIDE, SW_SHOW, WM_APP, WM_CLOSE, WM_COMMAND, WM_CTLCOLORBTN, WM_CTLCOLOREDIT,
    WM_CTLCOLORSTATIC, WM_DESTROY, WM_DRAWITEM, WM_PAINT, WM_SETFONT, WNDCLASSEXW, WS_CAPTION,
    WS_CHILD, WS_CLIPCHILDREN, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

#[cfg(windows)]
const SS_LEFT: u32 = 0x0000;
#[cfg(windows)]
const SS_NOPREFIX: u32 = 0x0080;
#[cfg(windows)]
const BM_GETCHECK: u32 = 0x00F0;
#[cfg(windows)]
const BM_SETCHECK: u32 = 0x00F1;
#[cfg(windows)]
const BST_CHECKED: usize = 1;
#[cfg(windows)]
const ES_AUTOHSCROLL: u32 = 0x0080;
#[cfg(windows)]
const ODS_SELECTED: u32 = 0x0001;
#[cfg(windows)]
const ODS_DISABLED: u32 = 0x0004;
#[cfg(windows)]
const ODS_FOCUS: u32 = 0x0010;

#[cfg(windows)]
const ID_BTN_PRIMARY: usize = 1001;
#[cfg(windows)]
const ID_BTN_SECONDARY: usize = 1002;
#[cfg(windows)]
const ID_BTN_BROWSE: usize = 1003;
#[cfg(windows)]
const ID_CHK_DESKTOP: usize = 1004;
#[cfg(windows)]
const ID_EDT_DIR: usize = 1005;

#[cfg(windows)]
const WM_INSTALL_PROGRESS: u32 = WM_APP + 1;
#[cfg(windows)]
const WM_INSTALL_SUCCESS: u32 = WM_APP + 2;
#[cfg(windows)]
const WM_INSTALL_ERROR: u32 = WM_APP + 3;

#[cfg(windows)]
fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

/// Resolves the user's real Desktop directory (supports OneDrive Desktop redirection via Known Folders).
pub fn desktop_dir() -> PathBuf {
    #[cfg(windows)]
    unsafe {
        if let Ok(pwstr) = SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None) {
            let result = pwstr.to_string().ok().map(PathBuf::from);
            CoTaskMemFree(Some(pwstr.0 as *const _));
            if let Some(p) = result {
                return p;
            }
        }
    }
    env::var_os("USERPROFILE")
        .map(|p| PathBuf::from(p).join("Desktop"))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Resolves the user's Start Menu Programs directory via Known Folders.
pub fn start_menu_programs_dir() -> PathBuf {
    #[cfg(windows)]
    unsafe {
        if let Ok(pwstr) = SHGetKnownFolderPath(&FOLDERID_Programs, KF_FLAG_DEFAULT, None) {
            let result = pwstr.to_string().ok().map(PathBuf::from);
            CoTaskMemFree(Some(pwstr.0 as *const _));
            if let Some(p) = result {
                return p;
            }
        }
    }
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Microsoft\\Windows\\Start Menu\\Programs")
}

pub fn desktop_shortcut_path() -> PathBuf {
    desktop_dir().join("Mutil-Zalo.lnk")
}

pub fn start_menu_shortcut_path() -> PathBuf {
    start_menu_programs_dir().join("Mutil-Zalo.lnk")
}

/// Appends `Mutil-Zalo` when the user selects a generic parent folder (e.g. `D:\Apps` -> `D:\Apps\Mutil-Zalo`).
pub fn normalize_picked_install_dir(picked: &Path) -> PathBuf {
    let already_named = picked
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.eq_ignore_ascii_case("Mutil-Zalo"))
        .unwrap_or(false);
    if already_named {
        picked.to_path_buf()
    } else {
        picked.join("Mutil-Zalo")
    }
}

/// Reads the previously registered InstallLocation from HKCU Uninstall registry, if any.
#[cfg(windows)]
pub fn registered_install_dir() -> Option<PathBuf> {
    unsafe {
        let subkey =
            to_wide_null("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Mutil-Zalo");
        let value = to_wide_null("InstallLocation");
        let mut hkey: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) != 0 {
            return None;
        }
        let mut byte_len: u32 = 0;
        let query_len = RegQueryValueExW(
            hkey,
            value.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut byte_len,
        );
        if query_len != 0 || byte_len < 4 {
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
pub fn registered_install_dir() -> Option<PathBuf> {
    None
}

pub fn default_install_dir() -> PathBuf {
    registered_install_dir().unwrap_or_else(install_dir)
}

/// Opens the modern Windows folder picker dialog (`IFileOpenDialog` with `FOS_PICKFOLDERS`).
#[cfg(windows)]
fn browse_for_folder(owner: HWND, current_dir: &Path) -> Option<PathBuf> {
    unsafe {
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

            dialog
                .Show(Some(windows::Win32::Foundation::HWND(owner as *mut _)))
                .ok()?;
            let item = dialog.GetResult().ok()?;
            let pwstr = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
            let path_str = pwstr.to_string().ok();
            CoTaskMemFree(Some(pwstr.0 as *const _));
            path_str.map(PathBuf::from)
        })();

        if should_uninit {
            CoUninitialize();
        }
        picked
    }
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca
            .to_string_lossy()
            .eq_ignore_ascii_case(&cb.to_string_lossy()),
        _ => a
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy()),
    }
}

/// Returns the expected installed executable path (from Registry InstallLocation if present).
pub fn active_installed_exe() -> Option<PathBuf> {
    registered_install_dir().map(|d| d.join("Mutil-Zalo.exe"))
}

/// Determines whether launching this single-file binary should open the installation wizard.
/// - Opening `Mutil-Zalo.exe` from outside the installed folder opens the Installer Wizard.
/// - Opening the installed `Mutil-Zalo.exe` (via Desktop / Start Menu shortcut) launches the app directly.
pub fn should_launch_installer(
    exe_path: &Path,
    installed_exe: Option<&Path>,
    args: &[String],
) -> bool {
    if args.iter().any(|a| {
        a.eq_ignore_ascii_case("--uninstall")
            || a.eq_ignore_ascii_case("--run")
            || a.eq_ignore_ascii_case("--portable")
    }) {
        return false;
    }
    if args.iter().any(|a| a.eq_ignore_ascii_case("--install")) {
        return true;
    }
    if let Some(inst) = installed_exe {
        if paths_equal(exe_path, inst) {
            return false;
        }
    }
    true
}

/// Creates a Windows `.lnk` shortcut natively using COM (`IShellLinkW` + `IPersistFile`).
/// Instant, zero PowerShell dependency, and 100% Unicode-safe for Vietnamese paths/descriptions.
#[cfg(windows)]
pub fn create_lnk_shortcut(
    link_path: &Path,
    target_exe: &Path,
    working_dir: &Path,
    icon_path: &Path,
    description: &str,
) -> Result<(), String> {
    if let Some(parent) = link_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Không thể tạo thư mục chứa shortcut: {e}"))?;
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
                .SetIconLocation(&HSTRING::from(icon_path.as_os_str()), 0)
                .map_err(|e| format!("SetIconLocation thất bại: {e}"))?;

            let persist_file: IPersistFile = shell_link
                .cast()
                .map_err(|e| format!("Không thể lấy IPersistFile: {e}"))?;
            persist_file
                .Save(&HSTRING::from(link_path.as_os_str()), true)
                .map_err(|e| format!("Không thể lưu shortcut {}: {e}", link_path.display()))?;
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
    _icon_path: &Path,
    _description: &str,
) -> Result<(), String> {
    Ok(())
}

pub fn create_desktop_shortcut(dst_exe: &Path) -> Result<(), String> {
    let workdir = dst_exe.parent().unwrap_or_else(|| Path::new("."));
    create_lnk_shortcut(
        &desktop_shortcut_path(),
        dst_exe,
        workdir,
        dst_exe,
        "Mutil-Zalo - Zalo Web Đa Tài Khoản",
    )
}

pub fn create_start_menu_shortcut(dst_exe: &Path) -> Result<(), String> {
    let workdir = dst_exe.parent().unwrap_or_else(|| Path::new("."));
    create_lnk_shortcut(
        &start_menu_shortcut_path(),
        dst_exe,
        workdir,
        dst_exe,
        "Mutil-Zalo - Zalo Web Đa Tài Khoản",
    )
}

#[cfg(windows)]
fn register_uninstall_entry(dir: &Path, dst_exe: &Path) -> Result<(), String> {
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
            return Err(format!("Không thể tạo mục Gỡ cài đặt Windows (mã {status})"));
        }

        let pairs = [
            ("DisplayName", "Mutil-Zalo (Zalo Web Đa Tài Khoản)".to_string()),
            ("DisplayVersion", env!("CARGO_PKG_VERSION").to_string()),
            ("Publisher", "Mutil-Zalo".to_string()),
            ("InstallLocation", dir.to_string_lossy().to_string()),
            ("DisplayIcon", format!("{},0", dst_exe.to_string_lossy())),
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
fn register_uninstall_entry(_dir: &Path, _dst_exe: &Path) -> Result<(), String> {
    Ok(())
}

/// Core installation steps into `target_dir` (copies exe, unpacks runtime, creates Start Menu shortcut & Uninstall entry).
/// Desktop shortcut is handled on the Finish screen so the user can choose.
fn perform_core_install<F>(target_dir: &Path, mut on_progress: F) -> Result<PathBuf, String>
where
    F: FnMut(u32, &str),
{
    on_progress(15, "Đang kiểm tra môi trường cài đặt...");
    if !ensure_webview2() {
        return Err("Chưa cài đặt Microsoft Edge WebView2 Runtime. Vui lòng thử lại.".to_string());
    }

    on_progress(40, "Đang sao chép tệp ứng dụng...");
    fs::create_dir_all(target_dir)
        .map_err(|e| format!("Không thể tạo thư mục cài đặt {}: {e}", target_dir.display()))?;
    let dst_exe = target_dir.join("Mutil-Zalo.exe");

    if let Ok(src) = env::current_exe() {
        if src.canonicalize().ok() != dst_exe.canonicalize().ok() {
            fs::copy(&src, &dst_exe)
                .map_err(|e| format!("Không thể sao chép tệp ứng dụng: {e}"))?;
        }
    }

    if HAS_REAL_PAYLOAD {
        on_progress(72, "Đang giải nén thành phần chạy ứng dụng...");
        let _ = unpack_runtime_if_needed()?;
    }

    on_progress(92, "Đang hoàn tất thiết lập...");
    create_start_menu_shortcut(&dst_exe)?;
    register_uninstall_entry(target_dir, &dst_exe)?;

    on_progress(100, "Cài đặt hoàn tất!");
    Ok(dst_exe)
}

/// Runs unattended CLI installation (`--install --silent`).
pub fn run_silent_install(args: &[String]) {
    let skip_desktop = args.iter().any(|a| {
        a.eq_ignore_ascii_case("--no-desktop") || a.eq_ignore_ascii_case("--no-desktop-shortcut")
    });
    let dir = args
        .windows(2)
        .find(|w| w[0].eq_ignore_ascii_case("--dir"))
        .map(|w| PathBuf::from(&w[1]))
        .unwrap_or_else(default_install_dir);

    if let Ok(dst_exe) = perform_core_install(&dir, |_, _| {}) {
        if !skip_desktop {
            let _ = create_desktop_shortcut(&dst_exe);
        }
    }
}

pub fn run_cli_uninstall() {
    let dir = registered_install_dir().unwrap_or_else(install_dir);
    let _ = fs::remove_file(desktop_shortcut_path());
    let _ = fs::remove_file(start_menu_shortcut_path());

    #[cfg(windows)]
    unsafe {
        let run_key = to_wide_null("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let val = to_wide_null("Mutil-Zalo");
        let mut hkey: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            run_key.as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut hkey,
        ) == 0
        {
            let _ = RegDeleteValueW(hkey, val.as_ptr());
            RegCloseKey(hkey);
        }
        let uninst =
            to_wide_null("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Mutil-Zalo");
        let _ = RegDeleteTreeW(HKEY_CURRENT_USER, uninst.as_ptr());
    }

    let _ = fs::remove_dir_all(runtime_root());

    // Schedule removal of installed files after process exits
    #[cfg(windows)]
    {
        let exe = dir.join("Mutil-Zalo.exe");
        let ico = dir.join("mutil-zalo.ico");
        let cmd = format!(
            "ping 127.0.0.1 -n 2 > nul & del /f /q \"{}\" \"{}\" 2>nul & rmdir \"{}\" 2>nul",
            exe.display(),
            ico.display(),
            dir.display()
        );
        let _ = Command::new("cmd.exe")
            .args(["/C", &cmd])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }

    crate::msg_box(
        "Mutil-Zalo",
        "Đã gỡ cài đặt Mutil-Zalo khỏi Windows.\n(Dữ liệu phiên Zalo trong AppData được giữ an toàn.)",
        windows_sys::Win32::UI::WindowsAndMessaging::MB_OK
            | windows_sys::Win32::UI::WindowsAndMessaging::MB_ICONINFORMATION,
    );
}

#[cfg(windows)]
#[derive(Copy, Clone, PartialEq, Eq)]
enum WizardStage {
    Ready,
    Installing,
    Finished,
    Error,
}

#[cfg(windows)]
struct WizardState {
    stage: WizardStage,
    dpi: i32,
    client_w: i32,
    client_h: i32,
    header_title: String,
    header_subtitle: String,
    selected_dir: PathBuf,
    installed_exe: Option<PathBuf>,

    icon_large: HICON,
    font_title: HFONT,
    font_subtitle: HFONT,
    font_bold: HFONT,
    font_regular: HFONT,
    brush_white: HBRUSH,
    brush_footer: HBRUSH,
    brush_border: HBRUSH,

    // Stage 1 (Ready — Choose folder & Install)
    lbl_dir_caption: HWND,
    edt_dir: HWND,
    btn_browse: HWND,

    // Stage 2 (Installing)
    lbl_progress_status: HWND,
    progress_bar: HWND,

    // Stage 3 (Finished — Desktop shortcut checkbox + Finish button)
    lbl_done_msg: HWND,
    chk_desktop: HWND,

    // Stage 4 (Error)
    lbl_error: HWND,

    // Footer buttons
    btn_primary: HWND,
    btn_secondary: HWND,
}

#[cfg(windows)]
impl WizardState {
    fn s(&self, px: i32) -> i32 {
        (px * self.dpi + 48) / 96
    }

    unsafe fn show(hwnd: HWND, visible: bool) {
        if !hwnd.is_null() {
            ShowWindow(hwnd, if visible { SW_SHOW } else { SW_HIDE });
        }
    }

    unsafe fn set_text(hwnd: HWND, text: &str) {
        if !hwnd.is_null() {
            let w = to_wide_null(text);
            SetWindowTextW(hwnd, w.as_ptr());
        }
    }

    unsafe fn get_text(hwnd: HWND) -> String {
        if hwnd.is_null() {
            return String::new();
        }
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let copied = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..copied.max(0) as usize])
    }

    unsafe fn position_footer_buttons(&self, single_primary: bool) {
        let footer_btn_y = self.client_h - self.s(48);
        let btn_h = self.s(34);
        let btn_primary_w = self.s(112);
        let btn_secondary_w = self.s(92);
        let right_margin = self.s(24);
        let gap = self.s(10);

        if single_primary {
            let x = self.client_w - right_margin - btn_primary_w;
            SetWindowPos(
                self.btn_primary,
                std::ptr::null_mut(),
                x,
                footer_btn_y,
                btn_primary_w,
                btn_h,
                SWP_NOZORDER,
            );
        } else {
            let sec_x = self.client_w - right_margin - btn_secondary_w;
            let pri_x = sec_x - gap - btn_primary_w;
            SetWindowPos(
                self.btn_primary,
                std::ptr::null_mut(),
                pri_x,
                footer_btn_y,
                btn_primary_w,
                btn_h,
                SWP_NOZORDER,
            );
            SetWindowPos(
                self.btn_secondary,
                std::ptr::null_mut(),
                sec_x,
                footer_btn_y,
                btn_secondary_w,
                btn_h,
                SWP_NOZORDER,
            );
        }
    }

    unsafe fn apply_stage(&mut self, main_hwnd: HWND, stage: WizardStage) {
        self.stage = stage;

        let is_ready = stage == WizardStage::Ready;
        let is_installing = stage == WizardStage::Installing;
        let is_finished = stage == WizardStage::Finished;
        let is_error = stage == WizardStage::Error;

        Self::show(self.lbl_dir_caption, is_ready);
        Self::show(self.edt_dir, is_ready);
        Self::show(self.btn_browse, is_ready);

        Self::show(self.lbl_progress_status, is_installing);
        Self::show(self.progress_bar, is_installing);

        Self::show(self.lbl_done_msg, is_finished);
        Self::show(self.chk_desktop, is_finished);

        Self::show(self.lbl_error, is_error);

        match stage {
            WizardStage::Ready => {
                self.header_title = "Cài đặt Mutil-Zalo".to_string();
                self.header_subtitle = "Chọn thư mục cài đặt và bấm Cài đặt để bắt đầu".to_string();
                self.position_footer_buttons(false);
                Self::set_text(self.btn_primary, "Cài đặt");
                Self::set_text(self.btn_secondary, "Hủy");
                Self::show(self.btn_primary, true);
                Self::show(self.btn_secondary, true);
                EnableWindow(self.btn_primary, 1);
                EnableWindow(self.btn_secondary, 1);
                SetFocus(self.btn_primary);
            }
            WizardStage::Installing => {
                self.header_title = "Đang cài đặt Mutil-Zalo...".to_string();
                self.header_subtitle = "Vui lòng đợi trong giây lát".to_string();
                self.position_footer_buttons(true);
                Self::set_text(self.btn_primary, "Đang cài...");
                Self::show(self.btn_primary, true);
                Self::show(self.btn_secondary, false);
                EnableWindow(self.btn_primary, 0);
                SendMessageW(self.progress_bar, PBM_SETPOS, 5, 0);
            }
            WizardStage::Finished => {
                self.header_title = "Cài đặt hoàn tất".to_string();
                self.header_subtitle = "Mutil-Zalo đã được cài đặt thành công trên máy tính".to_string();
                let dir_display = self
                    .installed_exe
                    .as_ref()
                    .and_then(|p| p.parent())
                    .unwrap_or(&self.selected_dir);
                Self::set_text(
                    self.lbl_done_msg,
                    &format!("Thư mục cài đặt:\n{}", dir_display.display()),
                );
                SendMessageW(self.chk_desktop, BM_SETCHECK, BST_CHECKED, 0);

                self.position_footer_buttons(true);
                Self::set_text(self.btn_primary, "Finish");
                Self::show(self.btn_primary, true);
                Self::show(self.btn_secondary, false);
                EnableWindow(self.btn_primary, 1);
                SetFocus(self.btn_primary);
            }
            WizardStage::Error => {
                self.header_title = "Cài đặt chưa hoàn tất".to_string();
                self.header_subtitle = "Đã xảy ra lỗi trong quá trình cài đặt".to_string();
                self.position_footer_buttons(false);
                Self::set_text(self.btn_primary, "Thử lại");
                Self::set_text(self.btn_secondary, "Đóng");
                Self::show(self.btn_primary, true);
                Self::show(self.btn_secondary, true);
                EnableWindow(self.btn_primary, 1);
                EnableWindow(self.btn_secondary, 1);
                SetFocus(self.btn_primary);
            }
        }

        InvalidateRect(main_hwnd, std::ptr::null(), 1);
        UpdateWindow(main_hwnd);
    }

    unsafe fn handle_browse(&mut self, main_hwnd: HWND) {
        let current = Self::get_text(self.edt_dir);
        let current_path = if current.trim().is_empty() {
            self.selected_dir.clone()
        } else {
            PathBuf::from(current.trim())
        };
        if let Some(picked) = browse_for_folder(main_hwnd, &current_path) {
            let normalized = normalize_picked_install_dir(&picked);
            self.selected_dir = normalized.clone();
            Self::set_text(self.edt_dir, &normalized.to_string_lossy());
        }
    }

    unsafe fn start_install(&mut self, main_hwnd: HWND) {
        let typed = Self::get_text(self.edt_dir);
        let trimmed = typed.trim();
        if trimmed.is_empty() {
            let t = to_wide_null("Mutil-Zalo");
            let m = to_wide_null("Vui lòng chọn thư mục cài đặt hợp lệ.");
            MessageBoxW(main_hwnd, m.as_ptr(), t.as_ptr(), MB_OK | MB_ICONWARNING);
            return;
        }
        let target_dir = PathBuf::from(trimmed);
        self.selected_dir = target_dir.clone();

        self.apply_stage(main_hwnd, WizardStage::Installing);
        let hwnd_raw = main_hwnd as isize;
        std::thread::spawn(move || {
            let res = perform_core_install(&target_dir, |pct, msg| {
                let boxed = Box::into_raw(Box::new(msg.to_string()));
                PostMessageW(
                    hwnd_raw as HWND,
                    WM_INSTALL_PROGRESS,
                    pct as WPARAM,
                    boxed as LPARAM,
                );
            });

            match res {
                Ok(dst_exe) => {
                    let boxed = Box::into_raw(Box::new(dst_exe));
                    PostMessageW(hwnd_raw as HWND, WM_INSTALL_SUCCESS, 0, boxed as LPARAM);
                }
                Err(err) => {
                    let boxed = Box::into_raw(Box::new(err));
                    PostMessageW(hwnd_raw as HWND, WM_INSTALL_ERROR, 0, boxed as LPARAM);
                }
            }
        });
    }

    unsafe fn finish_and_close(&self, main_hwnd: HWND) {
        let create_desktop =
            SendMessageW(self.chk_desktop, BM_GETCHECK, 0, 0) as usize == BST_CHECKED;

        let dst_exe = self
            .installed_exe
            .clone()
            .unwrap_or_else(|| self.selected_dir.join("Mutil-Zalo.exe"));

        if create_desktop {
            if let Err(e) = create_desktop_shortcut(&dst_exe) {
                let t = to_wide_null("Mutil-Zalo — Cảnh báo tạo Shortcut");
                let m = to_wide_null(&format!(
                    "Đã cài đặt ứng dụng nhưng không thể tạo biểu tượng ngoài Desktop:\n{e}"
                ));
                MessageBoxW(main_hwnd, m.as_ptr(), t.as_ptr(), MB_OK | MB_ICONWARNING);
            }
        }

        // Launch Mutil-Zalo right after Finish like the standard Zalo installer
        let _ = Command::new(&dst_exe).spawn();
        DestroyWindow(main_hwnd);
    }

    unsafe fn draw_owner_button(&self, dis: &DRAWITEMSTRUCT) {
        let hdc = dis.hDC;
        let rc = dis.rcItem;
        let is_primary = dis.CtlID as usize == ID_BTN_PRIMARY;
        let is_pressed = (dis.itemState & ODS_SELECTED) != 0;
        let is_disabled = (dis.itemState & ODS_DISABLED) != 0;
        let is_focused = (dis.itemState & ODS_FOCUS) != 0;

        // Paint parent background behind rounded corners
        let parent_bg = if dis.CtlID as usize == ID_BTN_BROWSE {
            self.brush_white
        } else {
            self.brush_footer
        };
        FillRect(hdc, &rc, parent_bg);

        let (bg_color, border_color, text_color) = if is_primary {
            if is_disabled {
                (rgb(147, 197, 253), rgb(147, 197, 253), rgb(255, 255, 255))
            } else if is_pressed {
                (rgb(0, 74, 184), rgb(0, 74, 184), rgb(255, 255, 255))
            } else {
                (rgb(0, 104, 255), rgb(0, 104, 255), rgb(255, 255, 255))
            }
        } else if is_disabled {
            (rgb(248, 250, 252), rgb(226, 232, 240), rgb(148, 163, 184))
        } else if is_pressed {
            (rgb(226, 232, 240), rgb(148, 163, 184), rgb(15, 23, 42))
        } else if is_focused {
            (rgb(255, 255, 255), rgb(0, 104, 255), rgb(15, 23, 42))
        } else {
            (rgb(255, 255, 255), rgb(203, 213, 225), rgb(30, 41, 59))
        };

        let brush = CreateSolidBrush(bg_color);
        let pen = CreatePen(PS_SOLID, 1, border_color);
        let old_brush = SelectObject(hdc, brush as _);
        let old_pen = SelectObject(hdc, pen as _);

        let radius = self.s(8);
        RoundRect(hdc, rc.left, rc.top, rc.right, rc.bottom, radius, radius);

        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        DeleteObject(brush as _);
        DeleteObject(pen as _);

        // Draw button text centered
        let label = Self::get_text(dis.hwndItem);
        let label_w = to_wide_null(&label);
        let font = if is_primary {
            self.font_bold
        } else {
            self.font_regular
        };
        let old_font = SelectObject(hdc, font as _);
        SetBkMode(hdc, TRANSPARENT as i32);
        SetTextColor(hdc, text_color);
        let mut text_rc = rc;
        DrawTextW(
            hdc,
            label_w.as_ptr(),
            -1,
            &mut text_rc,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        SelectObject(hdc, old_font);
    }
}

#[cfg(windows)]
impl Drop for WizardState {
    fn drop(&mut self) {
        unsafe {
            if !self.icon_large.is_null() {
                DestroyIcon(self.icon_large);
            }
            for f in [
                self.font_title,
                self.font_subtitle,
                self.font_bold,
                self.font_regular,
            ] {
                if !f.is_null() {
                    DeleteObject(f as _);
                }
            }
            for b in [self.brush_white, self.brush_footer, self.brush_border] {
                if !b.is_null() {
                    DeleteObject(b as _);
                }
            }
        }
    }
}

#[cfg(windows)]
unsafe fn create_ui_font(dpi: i32, pt: i32, weight: u32) -> HFONT {
    let height = -((pt * dpi + 36) / 72);
    let face = to_wide_null("Segoe UI");
    CreateFontW(
        height,
        0,
        0,
        0,
        weight as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_QUALITY as u32,
        (DEFAULT_PITCH | FF_SWISS) as u32,
        face.as_ptr(),
    )
}

#[cfg(windows)]
unsafe fn create_child(
    hinstance: HINSTANCE,
    parent: HWND,
    class: &str,
    text: &str,
    style: u32,
    ex_style: u32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    id: usize,
    font: HFONT,
) -> HWND {
    let cls = to_wide_null(class);
    let txt = to_wide_null(text);
    let hwnd = CreateWindowExW(
        ex_style,
        cls.as_ptr(),
        txt.as_ptr(),
        style,
        x,
        y,
        w,
        h,
        parent,
        id as _,
        hinstance,
        std::ptr::null(),
    );
    if !hwnd.is_null() && !font.is_null() {
        SendMessageW(hwnd, WM_SETFONT, font as WPARAM, 1);
    }
    hwnd
}

#[cfg(windows)]
unsafe extern "system" fn wizard_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WizardState;

    match msg {
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            if !state_ptr.is_null() {
                let st = &*state_ptr;
                let hdc = wparam as HDC;
                SetTextColor(hdc, rgb(30, 41, 59));
                SetBkColor(hdc, rgb(255, 255, 255));
                return st.brush_white as LRESULT;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_CTLCOLOREDIT => {
            if !state_ptr.is_null() {
                let st = &*state_ptr;
                let hdc = wparam as HDC;
                SetTextColor(hdc, rgb(15, 23, 42));
                SetBkColor(hdc, rgb(255, 255, 255));
                return st.brush_white as LRESULT;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_DRAWITEM => {
            if !state_ptr.is_null() && lparam != 0 {
                let st = &*state_ptr;
                let dis = &*(lparam as *const DRAWITEMSTRUCT);
                st.draw_owner_button(dis);
                return 1;
            }
            0
        }
        WM_PAINT => {
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut ps);
            if !state_ptr.is_null() {
                let st = &*state_ptr;
                let mut rc: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut rc);

                let header_h = st.s(82);
                let footer_y = rc.bottom - st.s(62);

                // Clean white main body
                let body_rc = RECT {
                    left: 0,
                    top: 0,
                    right: rc.right,
                    bottom: footer_y,
                };
                FillRect(hdc, &body_rc, st.brush_white);

                // Soft footer bar
                let footer_rc = RECT {
                    left: 0,
                    top: footer_y,
                    right: rc.right,
                    bottom: rc.bottom,
                };
                FillRect(hdc, &footer_rc, st.brush_footer);

                // Subtle 1px footer top divider
                let sep_footer = RECT {
                    left: 0,
                    top: footer_y,
                    right: rc.right,
                    bottom: footer_y + 1,
                };
                FillRect(hdc, &sep_footer, st.brush_border);

                // Zalo logo icon in header
                let icon_size = st.s(46);
                if !st.icon_large.is_null() {
                    DrawIconEx(
                        hdc,
                        st.s(24),
                        (header_h - icon_size) / 2 + st.s(2),
                        st.icon_large,
                        icon_size,
                        icon_size,
                        0,
                        std::ptr::null_mut(),
                        DI_NORMAL,
                    );
                }

                // Header title & subtitle text
                SetBkMode(hdc, TRANSPARENT as i32);

                let old_font = SelectObject(hdc, st.font_title as _);
                SetTextColor(hdc, rgb(15, 23, 42));
                let mut title_rc = RECT {
                    left: st.s(82),
                    top: st.s(18),
                    right: rc.right - st.s(20),
                    bottom: st.s(44),
                };
                let title_w = to_wide_null(&st.header_title);
                DrawTextW(
                    hdc,
                    title_w.as_ptr(),
                    -1,
                    &mut title_rc,
                    DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
                );

                SelectObject(hdc, st.font_subtitle as _);
                SetTextColor(hdc, rgb(100, 116, 139));
                let mut sub_rc = RECT {
                    left: st.s(82),
                    top: st.s(45),
                    right: rc.right - st.s(20),
                    bottom: st.s(66),
                };
                let sub_w = to_wide_null(&st.header_subtitle);
                DrawTextW(
                    hdc,
                    sub_w.as_ptr(),
                    -1,
                    &mut sub_rc,
                    DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
                );

                SelectObject(hdc, old_font);
            }
            EndPaint(hwnd, &ps);
            0
        }
        WM_COMMAND => {
            if !state_ptr.is_null() {
                let st = &mut *state_ptr;
                let id = wparam & 0xffff;
                match id {
                    ID_BTN_BROWSE => {
                        if st.stage == WizardStage::Ready {
                            st.handle_browse(hwnd);
                        }
                    }
                    ID_BTN_PRIMARY | 1 => match st.stage {
                        WizardStage::Ready | WizardStage::Error => {
                            st.start_install(hwnd);
                        }
                        WizardStage::Finished => {
                            st.finish_and_close(hwnd);
                        }
                        WizardStage::Installing => {}
                    },
                    ID_BTN_SECONDARY | 2 => match st.stage {
                        WizardStage::Ready | WizardStage::Error => {
                            DestroyWindow(hwnd);
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
            0
        }
        WM_INSTALL_PROGRESS => {
            if !state_ptr.is_null() && lparam != 0 {
                let st = &mut *state_ptr;
                let msg = Box::from_raw(lparam as *mut String);
                let pct = (wparam as usize).min(100);
                WizardState::set_text(st.lbl_progress_status, &msg);
                SendMessageW(st.progress_bar, PBM_SETPOS, pct, 0);
            }
            0
        }
        WM_INSTALL_SUCCESS => {
            if !state_ptr.is_null() && lparam != 0 {
                let st = &mut *state_ptr;
                let dst_exe = Box::from_raw(lparam as *mut PathBuf);
                st.installed_exe = Some(*dst_exe);
                SendMessageW(st.progress_bar, PBM_SETPOS, 100, 0);
                st.apply_stage(hwnd, WizardStage::Finished);
            }
            0
        }
        WM_INSTALL_ERROR => {
            if !state_ptr.is_null() && lparam != 0 {
                let st = &mut *state_ptr;
                let err = Box::from_raw(lparam as *mut String);
                WizardState::set_text(st.lbl_error, &format!("Lỗi: {err}"));
                st.apply_stage(hwnd, WizardStage::Error);
            }
            0
        }
        WM_CLOSE => {
            if !state_ptr.is_null() {
                let st = &*state_ptr;
                if st.stage == WizardStage::Installing {
                    return 0;
                }
            }
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            if !state_ptr.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(state_ptr));
            }
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Launches the modern native Win32 GUI Installer Wizard for Mutil-Zalo.
#[cfg(windows)]
pub fn run_gui_installer() {
    unsafe {
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES | ICC_PROGRESS_CLASS,
        };
        InitCommonControlsEx(&icc);

        let hinstance = GetModuleHandleW(std::ptr::null());

        let screen_dc = GetDC(std::ptr::null_mut());
        let dpi = if !screen_dc.is_null() {
            let d = GetDeviceCaps(screen_dc, LOGPIXELSX as i32);
            ReleaseDC(std::ptr::null_mut(), screen_dc);
            if d > 0 { d } else { 96 }
        } else {
            96
        };
        let s = |px: i32| (px * dpi + 48) / 96;

        let class_name = to_wide_null("MutilZaloInstallerWizardClass");
        let icon_small = LoadImageW(
            hinstance,
            1usize as *const u16,
            IMAGE_ICON,
            s(16),
            s(16),
            LR_DEFAULTCOLOR,
        ) as HICON;
        let icon_app = LoadImageW(
            hinstance,
            1usize as *const u16,
            IMAGE_ICON,
            s(32),
            s(32),
            LR_DEFAULTCOLOR,
        ) as HICON;
        let icon_large = LoadImageW(
            hinstance,
            1usize as *const u16,
            IMAGE_ICON,
            s(48),
            s(48),
            LR_DEFAULTCOLOR,
        ) as HICON;

        let brush_white = CreateSolidBrush(rgb(255, 255, 255));
        let brush_footer = CreateSolidBrush(rgb(248, 250, 252));
        let brush_border = CreateSolidBrush(rgb(226, 232, 240));

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wizard_wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: icon_app,
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            hbrBackground: brush_white,
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: icon_small,
        };
        RegisterClassExW(&wc);

        let client_w = s(480);
        let client_h = s(272);
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: client_w,
            bottom: client_h,
        };
        AdjustWindowRectEx(&mut rect, style, 0, 0);
        let win_w = rect.right - rect.left;
        let win_h = rect.bottom - rect.top;
        let x = (GetSystemMetrics(SM_CXSCREEN) - win_w) / 2;
        let y = (GetSystemMetrics(SM_CYSCREEN) - win_h) / 2;

        let title = to_wide_null("Cài đặt Mutil-Zalo");
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            style,
            x,
            y,
            win_w,
            win_h,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            let t = to_wide_null("Mutil-Zalo");
            let m = to_wide_null("Không thể mở cửa sổ trình cài đặt.");
            MessageBoxW(
                std::ptr::null_mut(),
                m.as_ptr(),
                t.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
            return;
        }

        let font_title = create_ui_font(dpi, 14, FW_BOLD);
        let font_subtitle = create_ui_font(dpi, 9, FW_NORMAL);
        let font_bold = create_ui_font(dpi, 10, FW_SEMIBOLD);
        let font_regular = create_ui_font(dpi, 10, FW_NORMAL);

        let margin_x = s(24);
        let content_w = client_w - margin_x * 2;
        let initial_dir = default_install_dir();
        let initial_dir_str = initial_dir.to_string_lossy().to_string();

        // Stage 1: Folder picker (clean, zero clutter)
        let lbl_dir_caption = create_child(
            hinstance,
            hwnd,
            "STATIC",
            "Thư mục cài đặt",
            WS_CHILD | WS_VISIBLE | SS_LEFT | SS_NOPREFIX,
            0,
            margin_x,
            s(102),
            content_w,
            s(22),
            0,
            font_bold,
        );
        let browse_w = s(86);
        let browse_gap = s(8);
        let edt_w = content_w - browse_w - browse_gap;
        let edt_dir = create_child(
            hinstance,
            hwnd,
            "EDIT",
            &initial_dir_str,
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL,
            0x0000_0200, // WS_EX_CLIENTEDGE
            margin_x,
            s(128),
            edt_w,
            s(30),
            ID_EDT_DIR,
            font_regular,
        );
        let btn_browse = create_child(
            hinstance,
            hwnd,
            "BUTTON",
            "Chọn...",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | (BS_OWNERDRAW as u32),
            0,
            margin_x + edt_w + browse_gap,
            s(128),
            browse_w,
            s(30),
            ID_BTN_BROWSE,
            font_regular,
        );

        // Stage 2: Installing progress
        let lbl_progress_status = create_child(
            hinstance,
            hwnd,
            "STATIC",
            "Đang cài đặt...",
            WS_CHILD | SS_LEFT | SS_NOPREFIX,
            0,
            margin_x,
            s(108),
            content_w,
            s(24),
            0,
            font_bold,
        );
        let progress_bar = create_child(
            hinstance,
            hwnd,
            "msctls_progress32",
            "",
            WS_CHILD,
            0,
            margin_x,
            s(140),
            content_w,
            s(20),
            0,
            std::ptr::null_mut(),
        );
        SendMessageW(progress_bar, PBM_SETRANGE, 0, (100isize) << 16);

        // Stage 3: Finished (clean — ONLY Desktop shortcut option + Finish button!)
        let lbl_done_msg = create_child(
            hinstance,
            hwnd,
            "STATIC",
            "",
            WS_CHILD | SS_LEFT | SS_NOPREFIX,
            0,
            margin_x,
            s(96),
            content_w,
            s(42),
            0,
            font_regular,
        );
        let chk_desktop = create_child(
            hinstance,
            hwnd,
            "BUTTON",
            "Tạo biểu tượng (Shortcut) ngoài Desktop",
            WS_CHILD | WS_TABSTOP | (BS_AUTOCHECKBOX as u32),
            0,
            margin_x,
            s(154),
            content_w,
            s(26),
            ID_CHK_DESKTOP,
            font_bold,
        );

        // Stage 4: Error
        let lbl_error = create_child(
            hinstance,
            hwnd,
            "STATIC",
            "",
            WS_CHILD | SS_LEFT | SS_NOPREFIX,
            0,
            margin_x,
            s(102),
            content_w,
            s(90),
            0,
            font_regular,
        );

        // Footer buttons (owner-drawn modern Zalo Blue primary + clean secondary)
        let btn_secondary = create_child(
            hinstance,
            hwnd,
            "BUTTON",
            "Hủy",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | (BS_OWNERDRAW as u32),
            0,
            0,
            0,
            s(92),
            s(34),
            ID_BTN_SECONDARY,
            font_regular,
        );
        let btn_primary = create_child(
            hinstance,
            hwnd,
            "BUTTON",
            "Cài đặt",
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | (BS_OWNERDRAW as u32),
            0,
            0,
            0,
            s(112),
            s(34),
            ID_BTN_PRIMARY,
            font_bold,
        );

        let mut state = Box::new(WizardState {
            stage: WizardStage::Ready,
            dpi,
            client_w,
            client_h,
            header_title: String::new(),
            header_subtitle: String::new(),
            selected_dir: initial_dir,
            installed_exe: None,
            icon_large,
            font_title,
            font_subtitle,
            font_bold,
            font_regular,
            brush_white,
            brush_footer,
            brush_border,
            lbl_dir_caption,
            edt_dir,
            btn_browse,
            lbl_progress_status,
            progress_bar,
            lbl_done_msg,
            chk_desktop,
            lbl_error,
            btn_primary,
            btn_secondary,
        });

        state.apply_stage(hwnd, WizardStage::Ready);
        let raw_state = Box::into_raw(state);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, raw_state as isize);

        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

#[cfg(not(windows))]
pub fn run_gui_installer() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn detects_installer_invocation_for_single_exe() {
        let installed = Path::new("C:\\Users\\Test\\AppData\\Local\\Programs\\Mutil-Zalo\\Mutil-Zalo.exe");
        let downloaded = Path::new("C:\\Users\\Test\\Downloads\\Mutil-Zalo.exe");

        // Fresh download (not installed yet) -> opens installer wizard
        assert!(should_launch_installer(downloaded, None, &[]));
        // Downloaded copy outside install dir -> opens installer wizard
        assert!(should_launch_installer(downloaded, Some(installed), &[]));
        // Running from installed folder (via Desktop / Start Menu shortcut) -> launches app directly
        assert!(!should_launch_installer(installed, Some(installed), &[]));
        // Explicit --install always opens installer wizard
        assert!(should_launch_installer(
            installed,
            Some(installed),
            &["--install".into()]
        ));
        // Explicit --run / --portable skips installer
        assert!(!should_launch_installer(downloaded, None, &["--run".into()]));
        assert!(!should_launch_installer(
            downloaded,
            None,
            &["--portable".into()]
        ));
        // --uninstall never opens installer
        assert!(!should_launch_installer(
            downloaded,
            None,
            &["--uninstall".into()]
        ));
    }

    #[test]
    fn normalizes_picked_install_directory() {
        assert_eq!(
            normalize_picked_install_dir(Path::new("D:\\Apps")),
            PathBuf::from("D:\\Apps\\Mutil-Zalo")
        );
        assert_eq!(
            normalize_picked_install_dir(Path::new("D:\\Apps\\Mutil-Zalo")),
            PathBuf::from("D:\\Apps\\Mutil-Zalo")
        );
        assert_eq!(
            normalize_picked_install_dir(Path::new("D:\\mutil-zalo")),
            PathBuf::from("D:\\mutil-zalo")
        );
    }

    #[cfg(windows)]
    #[test]
    fn creates_real_windows_lnk_shortcut_via_com() {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let tmp = env::temp_dir().join(format!("mutil-zalo-lnk-test-{ts}"));
        let link = tmp.join("Mutil-Zalo.lnk");
        let target = env::current_exe().unwrap();
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
