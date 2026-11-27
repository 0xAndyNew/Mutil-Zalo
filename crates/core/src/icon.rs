use tao::window::Icon as WindowIcon;
use tray_icon::Icon as TrayIconImage;

/// Generates a crisp 64x64 RGBA icon for Mutil-Zalo (Zalo-blue rounded badge with white Z + dual dots).
pub fn generate_rgba(size: u32) -> Vec<u8> {
    let mut data = vec![0u8; (size * size * 4) as usize];
    let s = size as f32;
    let radius = s * 0.22;

    for y in 0..size {
        for x in 0..size {
            let fx = x as f32 + 0.5;
            let fy = y as f32 + 0.5;

            // Signed distance to rounded rectangle
            let cx = (fx - s * 0.5).abs() - (s * 0.5 - radius - 1.0);
            let cy = (fy - s * 0.5).abs() - (s * 0.5 - radius - 1.0);
            let dist = (cx.max(0.0).hypot(cy.max(0.0))) + cx.max(cy).min(0.0) - radius;

            let alpha = (0.5 - dist).clamp(0.0, 1.0);
            if alpha <= 0.0 {
                continue;
            }

            // Gradient background: #0078FF (top-left) -> #0052CC (bottom-right)
            let t = ((fx + fy) / (2.0 * s)).clamp(0.0, 1.0);
            let mut r = (0.0 * (1.0 - t) + 0.0 * t) as f32;
            let mut g = (132.0 * (1.0 - t) + 82.0 * t) as f32;
            let mut b = (255.0 * (1.0 - t) + 214.0 * t) as f32;

            // Normalized coordinates 0..1
            let nx = fx / s;
            let ny = fy / s;

            // White stylized "Z" glyph in the center
            let top_bar = ny >= 0.25 && ny <= 0.35 && nx >= 0.24 && nx <= 0.72;
            let bot_bar = ny >= 0.65 && ny <= 0.75 && nx >= 0.24 && nx <= 0.72;
            // Diagonal from (0.67, 0.31) to (0.29, 0.69)
            let diag_y = 0.31 + (0.67 - nx) * (0.69 - 0.31) / (0.67 - 0.29);
            let diag = nx >= 0.25 && nx <= 0.71 && ny >= 0.29 && ny <= 0.71 && (ny - diag_y).abs() <= 0.068;

            // Accent multi-account badge dot in top-right (emerald green #22C55E with white ring)
            let dot_dist = (nx - 0.77).hypot(ny - 0.23);

            if dot_dist <= 0.135 {
                if dot_dist <= 0.095 {
                    r = 34.0;
                    g = 211.0;
                    b = 110.0;
                } else {
                    r = 255.0;
                    g = 255.0;
                    b = 255.0;
                }
            } else if top_bar || bot_bar || diag {
                r = 255.0;
                g = 255.0;
                b = 255.0;
            }

            let idx = ((y * size + x) * 4) as usize;
            data[idx] = r.round() as u8;
            data[idx + 1] = g.round() as u8;
            data[idx + 2] = b.round() as u8;
            data[idx + 3] = (alpha * 255.0).round() as u8;
        }
    }

    data
}

pub fn window_icon() -> Option<WindowIcon> {
    let size = 64;
    WindowIcon::from_rgba(generate_rgba(size), size, size).ok()
}

pub fn tray_icon() -> Option<TrayIconImage> {
    let size = 32;
    TrayIconImage::from_rgba(generate_rgba(size), size, size).ok()
}

/// Generates a valid 48x48 32-bit BGRA Windows `.ico` file in memory (no external image crate needed).
pub fn generate_ico_bytes() -> Vec<u8> {
    let size: u32 = 48;
    let rgba = generate_rgba(size);
    let pixel_bytes = (size * size * 4) as u32;
    let mask_row_bytes = ((size + 31) / 32) * 4;
    let mask_bytes = mask_row_bytes * size;
    let dib_header_size: u32 = 40;
    let image_size = dib_header_size + pixel_bytes + mask_bytes;

    let mut out = Vec::with_capacity((6 + 16 + image_size) as usize);

    // ICONDIR header (6 bytes)
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type: 1 = ICO
    out.extend_from_slice(&1u16.to_le_bytes()); // count: 1

    // ICONDIRENTRY (16 bytes)
    out.push(size as u8); // width
    out.push(size as u8); // height
    out.push(0); // color count
    out.push(0); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&32u16.to_le_bytes()); // bit count
    out.extend_from_slice(&image_size.to_le_bytes()); // bytes in resource
    out.extend_from_slice(&22u32.to_le_bytes()); // offset (6 + 16)

    // BITMAPINFOHEADER (40 bytes)
    out.extend_from_slice(&dib_header_size.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes()); // width
    out.extend_from_slice(&((size * 2) as i32).to_le_bytes()); // height * 2 (XOR + AND mask)
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&32u16.to_le_bytes()); // bpp
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&(pixel_bytes + mask_bytes).to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    // Pixel data (bottom-up BGRA)
    for y in (0..size).rev() {
        for x in 0..size {
            let idx = ((y * size + x) * 4) as usize;
            let r = rgba[idx];
            let g = rgba[idx + 1];
            let b = rgba[idx + 2];
            let a = rgba[idx + 3];
            out.push(b);
            out.push(g);
            out.push(r);
            out.push(a);
        }
    }

    // AND mask (all zeros since 32-bit alpha channel is used)
    out.resize(out.len() + mask_bytes as usize, 0u8);

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_valid_ico_header() {
        let bytes = generate_ico_bytes();
        assert!(bytes.len() > 64);
        assert_eq!(&bytes[0..6], &[0, 0, 1, 0, 1, 0]);
        assert_eq!(bytes[6], 48);
        assert_eq!(bytes[7], 48);
    }
}
