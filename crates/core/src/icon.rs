use tao::window::Icon as WindowIcon;
use tray_icon::Icon as TrayIconImage;

const ZALO_64_RGBA: &[u8] = include_bytes!("../assets/zalo_64x64.rgba");
const ZALO_32_RGBA: &[u8] = include_bytes!("../assets/zalo_32x32.rgba");
const ZALO_ICO: &[u8] = include_bytes!("../assets/zalo.ico");

pub fn window_icon() -> Option<WindowIcon> {
    WindowIcon::from_rgba(ZALO_64_RGBA.to_vec(), 64, 64).ok()
}

pub fn tray_icon() -> Option<TrayIconImage> {
    TrayIconImage::from_rgba(ZALO_32_RGBA.to_vec(), 32, 32).ok()
}

/// Returns the embedded multi-resolution (16/32/48/64/128px) official Zalo `.ico` bytes.
pub fn generate_ico_bytes() -> Vec<u8> {
    ZALO_ICO.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_valid_ico_header() {
        let bytes = generate_ico_bytes();
        assert!(bytes.len() > 1000);
        // ICO header: reserved=0, type=1, count=5
        assert_eq!(&bytes[0..6], &[0, 0, 1, 0, 5, 0]);
        assert_eq!(ZALO_64_RGBA.len(), 64 * 64 * 4);
        assert_eq!(ZALO_32_RGBA.len(), 32 * 32 * 4);
    }
}
