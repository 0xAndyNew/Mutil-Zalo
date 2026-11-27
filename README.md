# Mutil-Zalo — Ứng Dụng Desktop Zalo Web Đa Tài Khoản (Pure Rust)

**Mutil-Zalo** là ứng dụng desktop nhẹ cho Windows 10/11 (64-bit) viết hoàn toàn bằng **Rust** (`tao` + `wry` + `tray-icon` + WinRT), đóng gói **Zalo Web (`https://chat.zalo.me`)** giúp đăng nhập và sử dụng **nhiều tài khoản Zalo song song** trong cùng một cửa sổ — đóng gói gọn gàng trong **duy nhất 1 file `Mutil-Zalo.exe` (~1 MB)**.

> **Tải bản cài đặt / chạy ngay (1 file `.exe` duy nhất):**
> 👉 [Tải Mutil-Zalo.exe tại trang Releases mới nhất](https://github.com/0xAndyNew/Mutil-Zalo/releases/latest)

---

## Tính Năng Nổi Bật

- **Đa tài khoản cô lập hoàn toàn — Chuyển đổi tức thì KHÔNG đăng xuất**:
  - Mỗi tài khoản Zalo chạy trên một hồ sơ WebView2 riêng (`%LOCALAPPDATA%\Mutil-Zalo\profiles\<id>\`), tách biệt 100% cookies, `localStorage`, `IndexedDB` và đăng nhập QR.
  - Chuyển đi chuyển lại giữa các tài khoản **tức thì (<10ms), không tải lại trang, không chớp trắng, không mất đoạn chat đang gõ**.
  - Tắt mở app hoặc khởi động lại Windows vẫn giữ nguyên phiên đăng nhập của tất cả tài khoản.
- **Thanh chuyển tài khoản gọn gàng & Tự động thu gọn**:
  - Thu gọn thanh chuyển nick thành mép kéo siêu mỏng `16px` trùng khớp màu xanh `#005AE0` của Zalo Web — liền mạch 1 khối, tránh rối mắt.
  - Hỗ trợ chế độ **Tự động thu gọn** (trong Cài đặt): mở thanh bằng mũi tên ở mép trái → chọn xong tài khoản là thanh tự ẩn gọn lại ngay.
  - Khi thu gọn vẫn hiển thị chấm đỏ nhỏ báo khi có tài khoản nền nhận tin nhắn mới.
- **Thông báo Windows gốc (Windows Toast Notification) như app thật**:
  - Nhận thông báo góc màn hình Windows 10/11 kèm âm thanh và nháy Taskbar cho **mọi tài khoản** (kể cả tài khoản đang chạy nền hoặc khi app ẩn xuống khay hệ thống).
  - **Bấm trực tiếp vào thông báo Windows** sẽ tự động bật app lên và chuyển thẳng sang đúng tài khoản vừa có tin nhắn mới.
- **Chạy ẩn ổn định trong Khay Hệ Thống (System Tray)**:
  - Bấm `X` hoặc nút thu xuống khay sẽ chạy ẩn nền, giữ kết nối WebView2 không bị đóng băng để liên tục nhận thông báo.
  - Bấm chuột phải vào icon khay hệ thống để chuyển nhanh tài khoản, ẩn/hiện cửa sổ, mở Cài đặt hoặc thoát hoàn toàn.
- **Đóng gói 1 file `Mutil-Zalo.exe` duy nhất (~1 MB) — chạy trên mọi máy Windows 10/11 (64-bit)**:
  - Không cần DLL rời bên cạnh, không cần cài Rust / Node.js / .NET / Visual C++ Redistributable, không cần quyền Admin.
  - Tự phát hiện và tự tải cài đặt **Microsoft Edge WebView2 Runtime** chính chủ từ Microsoft nếu máy đích chưa có.
  - Tích hợp sẵn nút **Cài đặt vào máy** (tạo Shortcut Desktop, Start Menu & mục Gỡ cài đặt trong Windows) ngay trong phần Cài đặt (`⚙`), hoặc qua dòng lệnh `--install` / `--uninstall`.

---

## Hướng Dẫn Sử Dụng Nhanh

1. Tải file duy nhất **`Mutil-Zalo.exe`** từ mục [Releases](https://github.com/0xAndyNew/Mutil-Zalo/releases/latest).
2. Bấm đúp để chạy ngay *(lần đầu mở trên máy mới nếu Windows SmartScreen hiện cảnh báo, chọn **More info → Run anyway**)*.
3. Quét mã QR đăng nhập tài khoản Zalo đầu tiên.
4. Bấm nút **`+`** hoặc **`⚙` Cài đặt** → **+ Thêm tài khoản mới**, đặt tên & màu nhận diện → quét mã QR cho tài khoản thứ 2, thứ 3...
5. Chuyển qua lại giữa các tài khoản bằng:
   - Click trực tiếp icon tài khoản trên thanh bên trái
   - Mở **`⚙` Cài đặt** → bấm **Chuyển sang**
   - Phím tắt nhanh **`Ctrl + 1`** đến **`Ctrl + 9`**
   - Menu chuột phải tại biểu tượng **System Tray**

### Bảng Phím Tắt

| Phím tắt | Chức năng |
|---|---|
| `Ctrl + 1` .. `Ctrl + 9` | Chuyển nhanh tới tài khoản Zalo thứ 1..9 (không đăng xuất) |
| `Ctrl + B` | Ẩn / Hiện thanh chuyển tài khoản bên trái |
| `Ctrl + ,` | Mở / Đóng bảng Cài đặt & Quản lý tài khoản |
| `Esc` | Đóng bảng Cài đặt, quay lại cửa sổ Zalo |

### Cài Đặt / Gỡ Cài Đặt Trên Windows

- **Trong ứng dụng**: Mở **`⚙` Cài đặt → tab Cài đặt ứng dụng → Cài đặt vào máy** (tạo Shortcut Desktop, Start Menu và mục gỡ cài đặt trong Windows Settings).
- **Qua dòng lệnh**:
  ```powershell
  .\Mutil-Zalo.exe --install
  .\Mutil-Zalo.exe --uninstall
  ```

---

## Kiến Trúc & Vị Trí Dữ Liệu

```text
Mutil-Zalo/
├── Cargo.toml                 # Cargo Workspace (mutil-zalo-core + mutil-zalo)
├── build.ps1                  # Script build 1 lệnh xuất ra duy nhất dist\Mutil-Zalo.exe
├── crates/
│   ├── core/                  # Ứng dụng chính (tao + wry + tray-icon + WinRT Toast)
│   │   └── src/
│   │       ├── main.rs        # Event loop, quản lý cửa sổ & điều phối IPC
│   │       ├── accounts.rs    # Hồ sơ WebView2 cô lập & chuyển đổi HWND 0ms không reload
│   │       ├── notify.rs      # Thông báo Windows Toast gốc (WinRT) & click mở đúng tài khoản
│   │       ├── tray.rs        # Khay hệ thống & menu chuyển nick nhanh
│   │       ├── config.rs      # Lưu cấu hình JSON tại %LOCALAPPDATA%\Mutil-Zalo\config.json
│   │       ├── win_utils.rs   # Single-Instance Mutex + HWND restore, Startup Registry, Shortcut
│   │       ├── icon.rs        # Tạo icon RGBA & .ico thuần Rust
│   │       └── ui/            # Giao diện Sidebar & Cài đặt (nhúng trực tiếp include_str!)
│   └── launcher/              # Bộ đóng gói 1 file duy nhất (nén gzip core + WebView2Loader.dll)
└── dist/
    └── Mutil-Zalo.exe         # File thực thi duy nhất sau khi build
```

- **Thư mục lưu dữ liệu phiên & cấu hình**: `%LOCALAPPDATA%\Mutil-Zalo\`
- **Quyền riêng tư**: Ứng dụng chạy hoàn toàn cục bộ trên máy tính người dùng, kết nối trực tiếp tới `https://chat.zalo.me`, không thu thập hay gửi dữ liệu phiên ra bên ngoài.

---

## Tự Biên Dịch Từ Mã Nguồn (Build From Source)

Yêu cầu: **Rust** (khuyên dùng toolchain `stable-x86_64-pc-windows-gnullvm` + `stable-x86_64-pc-windows-gnu` cài qua `rustup`, không cần Visual Studio Build Tools).

```powershell
# Chạy unit tests
$tc = "$env:USERPROFILE\.rustup\toolchains"
$lld = "$tc\stable-x86_64-pc-windows-gnullvm\lib\rustlib\x86_64-pc-windows-gnullvm\bin\gcc-ld\ld.lld.exe"
$gnuLib = "$tc\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\lib\self-contained"
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER = $lld
$env:RUSTFLAGS = "-C link-self-contained=yes -C linker-flavor=ld -C link-arg=-m -C link-arg=i386pep -C link-arg=-L$gnuLib"
cargo +stable-x86_64-pc-windows-gnullvm test --workspace

# Build 1 file duy nhất -> dist\Mutil-Zalo.exe
powershell -ExecutionPolicy Bypass -File .\build.ps1
```
