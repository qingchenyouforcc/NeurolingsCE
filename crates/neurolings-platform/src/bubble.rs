//! 语气泡位图渲染（Windows GDI）：白底圆角矩形上绘制深色文本（可选粗体
//! 标题行），返回带逐像素 alpha 的预乘 BGRA——内容 DC 提供颜色，掩码 DC
//! 提供气泡轮廓。

#[cfg(windows)]
/// 将标题和正文渲染为预乘 BGRA 气泡位图。
pub fn render_bubble(
    title: &str,
    text: &str,
    max_width: u32,
) -> Result<(Vec<u8>, u32, u32), String> {
    use windows::Win32::Foundation::COLORREF;
    use windows::Win32::Graphics::Gdi::ReleaseDC;
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BLACK_BRUSH, CreateCompatibleDC, CreateDIBSection, CreateFontW,
        CreateSolidBrush, DIB_RGB_COLORS, DT_CALCRECT, DT_LEFT, DT_NOCLIP, DT_TOP, DT_WORDBREAK,
        DeleteDC, DeleteObject, DrawTextW, FillRect, GetDC, GetStockObject, RoundRect,
        SelectObject, SetBkColor, SetTextColor, WHITE_PEN,
    };
    use windows::core::PCWSTR;

    if text.trim().is_empty() {
        return Err("empty bubble text".into());
    }

    let font_height = 16i32;
    let padding = 10u32;
    let width = max_width.clamp(80, 360);

    unsafe {
        let screen_dc = GetDC(None);

        let make_dc = || -> Result<
            (
                windows::Win32::Graphics::Gdi::HDC,
                windows::Win32::Graphics::Gdi::HBITMAP,
                *const u8,
                windows::Win32::Graphics::Gdi::HGDIOBJ,
            ),
            String,
        > {
            let dc = CreateCompatibleDC(Some(screen_dc));
            let mut bmi = BITMAPINFO::default();
            bmi.bmiHeader.biSize = std::mem::size_of::<windows::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = width as i32;
            bmi.bmiHeader.biHeight = -240i32; // 自上而下，预留最大气泡高度
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB.0;
            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let hbmp = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
                .map_err(|e| e.to_string())?;
            if bits.is_null() {
                let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(hbmp.0));
                let _ = DeleteDC(dc);
                return Err("CreateDIBSection returned an empty pixel buffer".into());
            }
            let previous = SelectObject(dc, windows::Win32::Graphics::Gdi::HGDIOBJ(hbmp.0));
            Ok((dc, hbmp, bits.cast(), previous))
        };

        let (content_dc, content_bmp, content_bits, content_previous) = make_dc()?;
        let (mask_dc, mask_bmp, mask_bits, mask_previous) = match make_dc() {
            Ok(value) => value,
            Err(error) => {
                let _ = SelectObject(content_dc, content_previous);
                let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(content_bmp.0));
                let _ = DeleteDC(content_dc);
                let _ = ReleaseDC(None, screen_dc);
                return Err(error);
            }
        };

        // 测量换行后的文本高度。
        let wide: Vec<u16> = text.encode_utf16().collect();
        let mut rect = windows::Win32::Foundation::RECT {
            left: padding as i32,
            top: padding as i32,
            right: (width - padding) as i32,
            bottom: 200,
        };
        let font_name: Vec<u16> = "Segoe UI"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let font = CreateFontW(
            font_height,
            0,
            0,
            0,
            400i32,
            0,
            0,
            0,
            windows::Win32::Graphics::Gdi::FONT_CHARSET(1),
            windows::Win32::Graphics::Gdi::FONT_OUTPUT_PRECISION(0),
            windows::Win32::Graphics::Gdi::FONT_CLIP_PRECISION(0),
            windows::Win32::Graphics::Gdi::FONT_QUALITY(5),
            0,
            PCWSTR(font_name.as_ptr()),
        );
        let bold_font = CreateFontW(
            font_height,
            0,
            0,
            0,
            700i32,
            0,
            0,
            0,
            windows::Win32::Graphics::Gdi::FONT_CHARSET(1),
            windows::Win32::Graphics::Gdi::FONT_OUTPUT_PRECISION(0),
            windows::Win32::Graphics::Gdi::FONT_CLIP_PRECISION(0),
            windows::Win32::Graphics::Gdi::FONT_QUALITY(5),
            0,
            PCWSTR(font_name.as_ptr()),
        );
        let previous_font =
            SelectObject(content_dc, windows::Win32::Graphics::Gdi::HGDIOBJ(font.0));
        let height_needed = DrawTextW(
            content_dc,
            &mut wide.clone(),
            &mut rect,
            DT_CALCRECT | DT_LEFT | DT_TOP | DT_WORDBREAK,
        );
        let body_height = height_needed.max(font_height) as u32;
        // 可选标题行（粗体），置于正文之上，间隔 4px。
        let wide_title: Vec<u16> = title.encode_utf16().collect();
        let title_height = if wide_title.is_empty() {
            0u32
        } else {
            let _ = SelectObject(
                content_dc,
                windows::Win32::Graphics::Gdi::HGDIOBJ(bold_font.0),
            );
            let mut title_rect = windows::Win32::Foundation::RECT {
                left: padding as i32,
                top: padding as i32,
                right: (width - padding) as i32,
                bottom: 200,
            };
            let h = DrawTextW(
                content_dc,
                &mut wide_title.clone(),
                &mut title_rect,
                DT_CALCRECT | DT_LEFT | DT_TOP | DT_WORDBREAK,
            );
            let _ = SelectObject(content_dc, windows::Win32::Graphics::Gdi::HGDIOBJ(font.0));
            h.max(font_height) as u32
        };
        let title_gap = if title_height > 0 { 4u32 } else { 0 };
        let text_height = body_height + title_height + title_gap;
        let height = (text_height + padding * 2).clamp(40, 240);

        // 内容位图：白底黑字（只含颜色）。
        let white = CreateSolidBrush(COLORREF(0x00FF_FFFF));
        let fill_rect = windows::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: width as i32,
            bottom: height as i32,
        };
        let _ = FillRect(content_dc, &fill_rect, white);
        let _ = SetBkColor(content_dc, COLORREF(0x00FF_FFFF));
        let _ = SetTextColor(content_dc, COLORREF(0x0020_2020));
        let mut text_rect = windows::Win32::Foundation::RECT {
            left: padding as i32,
            top: padding as i32,
            right: (width - padding) as i32,
            bottom: (height - padding) as i32,
        };
        if title_height > 0 {
            let _ = SelectObject(
                content_dc,
                windows::Win32::Graphics::Gdi::HGDIOBJ(bold_font.0),
            );
            let mut title_rect = windows::Win32::Foundation::RECT {
                left: padding as i32,
                top: padding as i32,
                right: (width - padding) as i32,
                bottom: (padding + title_height) as i32,
            };
            let _ = DrawTextW(
                content_dc,
                &mut wide_title.clone(),
                &mut title_rect,
                DT_LEFT | DT_TOP | DT_WORDBREAK | DT_NOCLIP,
            );
            let _ = SelectObject(content_dc, windows::Win32::Graphics::Gdi::HGDIOBJ(font.0));
            text_rect.top = (padding + title_height + title_gap) as i32;
        }
        let _ = DrawTextW(
            content_dc,
            &mut wide.clone(),
            &mut text_rect,
            DT_LEFT | DT_TOP | DT_WORDBREAK | DT_NOCLIP,
        );

        // 掩码位图：黑底上的白色圆角矩形。
        let black_rect = windows::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: width as i32,
            bottom: height as i32,
        };
        let black = windows::Win32::Graphics::Gdi::HBRUSH(GetStockObject(BLACK_BRUSH).0);
        let _ = FillRect(mask_dc, &black_rect, black);
        let pen = windows::Win32::Graphics::Gdi::HPEN(GetStockObject(WHITE_PEN).0);
        let previous_pen = SelectObject(mask_dc, windows::Win32::Graphics::Gdi::HGDIOBJ(pen.0));
        let previous_brush = SelectObject(mask_dc, windows::Win32::Graphics::Gdi::HGDIOBJ(white.0));
        let _ = RoundRect(
            mask_dc,
            1,
            1,
            (width - 1) as i32,
            (height - 1) as i32,
            18,
            18,
        );

        // CreateDIBSection 已返回自上而下的像素缓冲区。不能用 GetDIBits 回读，
        // 因为它要求位图未被选入 DC，而绘制时位图必须保持选入。
        let pixel_len = (width * height * 4) as usize;
        let content_px = std::slice::from_raw_parts(content_bits, pixel_len);
        let mask_px = std::slice::from_raw_parts(mask_bits, pixel_len);

        // 合成：alpha 取掩码亮度、颜色取内容位图，再做预乘。
        let mut out = vec![0u8; (width * height * 4) as usize];
        for y in 0..height {
            for x in 0..width {
                let i = (y as usize * width as usize + x as usize) * 4;
                let o = (y as usize * width as usize + x as usize) * 4;
                let m = mask_px[i]; // 掩码的 B 通道（气泡内为白）
                if m < 8 {
                    continue; // 气泡外完全透明
                }
                let alpha = 235u32.min(m as u32 * 235 / 255);
                let b = content_px[i] as u32;
                let g = content_px[i + 1] as u32;
                let r = content_px[i + 2] as u32;
                out[o] = (b * alpha / 255) as u8;
                out[o + 1] = (g * alpha / 255) as u8;
                out[o + 2] = (r * alpha / 255) as u8;
                out[o + 3] = alpha as u8;
            }
        }

        // 先恢复 DC 的原对象，否则 DeleteObject 会因对象仍被选入而失败。
        let _ = SelectObject(content_dc, previous_font);
        let _ = SelectObject(mask_dc, previous_pen);
        let _ = SelectObject(mask_dc, previous_brush);
        let _ = SelectObject(content_dc, content_previous);
        let _ = SelectObject(mask_dc, mask_previous);
        let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(font.0));
        let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(bold_font.0));
        let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(white.0));
        let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(content_bmp.0));
        let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(mask_bmp.0));
        let _ = DeleteDC(content_dc);
        let _ = DeleteDC(mask_dc);
        let _ = ReleaseDC(None, screen_dc);

        Ok((out, width, height))
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::render_bubble;

    /// Windows GDI 路径必须产出可见的气泡轮廓和正文像素。
    #[test]
    fn windows_bubble_contains_surface_and_text() {
        let (bitmap, width, height) = render_bubble("Title", "Hello Windows", 260).unwrap();
        assert_eq!(bitmap.len(), (width * height * 4) as usize);
        assert!(bitmap.as_chunks::<4>().0.iter().any(|pixel| pixel[3] > 0));
        assert!(
            bitmap
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| { pixel[3] > 0 && pixel[0] < 64 && pixel[1] < 64 && pixel[2] < 64 })
        );
    }
}

#[cfg(not(windows))]
fn glyph(ch: char) -> [u8; 7] {
    match ch.to_ascii_uppercase() {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01111, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        '.' => [0, 0, 0, 0, 0, 0b00110, 0b00110],
        ',' => [0, 0, 0, 0, 0, 0b00110, 0b00100],
        ':' => [0, 0b00110, 0b00110, 0, 0b00110, 0b00110, 0],
        '!' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0, 0b00100],
        '?' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0, 0b00100],
        '-' => [0, 0, 0, 0b11111, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 0b11111],
        '(' => [
            0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010,
        ],
        ')' => [
            0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000,
        ],
        '/' => [
            0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000,
        ],
        '+' => [0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0],
        '=' => [0, 0b11111, 0, 0b11111, 0, 0, 0],
        _ => [
            0b11111, 0b10001, 0b10101, 0b10001, 0b10101, 0b10001, 0b11111,
        ],
    }
}

#[cfg(not(windows))]
fn wrap_lines(text: &str, max_chars: usize) -> Vec<String> {
    let max_chars = max_chars.max(1);
    let mut lines = Vec::new();
    for source in text.lines() {
        let chars: Vec<char> = source.chars().collect();
        if chars.is_empty() {
            lines.push(String::new());
            continue;
        }
        for chunk in chars.chunks(max_chars) {
            lines.push(chunk.iter().collect());
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(not(windows))]
fn draw_text_line(
    bitmap: &mut [u8],
    width: u32,
    height: u32,
    text: &str,
    x: u32,
    y: u32,
    bold: bool,
) {
    const SCALE: u32 = 2;
    const CELL_WIDTH: u32 = 12;
    const GLYPH_TOP: u32 = 1;
    for (column, ch) in text.chars().enumerate() {
        let pattern = glyph(ch);
        let origin_x = x + column as u32 * CELL_WIDTH;
        for (row, bits) in pattern.iter().enumerate() {
            for bit in 0..5u32 {
                if bits & (1 << (4 - bit)) == 0 {
                    continue;
                }
                let pixel_x = origin_x + bit * SCALE;
                let pixel_y = y + GLYPH_TOP + row as u32 * SCALE;
                for dy in 0..SCALE {
                    for dx in 0..SCALE {
                        let px = pixel_x + dx;
                        let py = pixel_y + dy;
                        if px >= width || py >= height {
                            continue;
                        }
                        let offset = ((py * width + px) * 4) as usize;
                        bitmap[offset..offset + 4].copy_from_slice(&[0, 0, 0, 255]);
                        if bold && px + 1 < width {
                            bitmap[offset + 4..offset + 8].copy_from_slice(&[0, 0, 0, 255]);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(not(windows))]
/// 使用无外部依赖的位图字形渲染非 Windows 平台气泡。
pub fn render_bubble(
    title: &str,
    text: &str,
    max_width: u32,
) -> Result<(Vec<u8>, u32, u32), String> {
    if text.trim().is_empty() {
        return Err("empty bubble text".into());
    }
    let padding = 10u32;
    let width = max_width.clamp(80, 360);
    let inner_width = width.saturating_sub(padding * 2);
    let chars_per_line = (inner_width / 12).max(1) as usize;
    let mut body_lines = wrap_lines(text, chars_per_line);
    let title_lines = if title.is_empty() {
        Vec::new()
    } else {
        wrap_lines(title, chars_per_line)
    };
    // 高度上限与 Windows GDI 路径一致，过长通知只保留可见的前缀。
    let max_lines = ((240u32.saturating_sub(padding * 2)) / 16).max(1) as usize;
    let title_count = title_lines.len().min(max_lines);
    body_lines.truncate(max_lines.saturating_sub(title_count));
    if body_lines.is_empty() {
        body_lines.push(String::new());
    }
    let title_gap = if title_count > 0 { 4 } else { 0 };
    let lines = title_count + body_lines.len();
    let height = (padding * 2 + lines as u32 * 16 + title_gap).clamp(40, 240);
    let mut bitmap = vec![0u8; (width * height * 4) as usize];
    let radius = 8i32;
    for y in 0..height {
        for x in 0..width {
            let edge_x = x.min(width - 1 - x) as i32;
            let edge_y = y.min(height - 1 - y) as i32;
            let inside_corner = edge_x >= radius
                || edge_y >= radius
                || (edge_x - radius).pow(2) + (edge_y - radius).pow(2) <= radius.pow(2);
            if inside_corner {
                let offset = ((y * width + x) * 4) as usize;
                bitmap[offset..offset + 4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
    }
    let mut line_y = padding;
    for line in title_lines.iter().take(title_count) {
        draw_text_line(&mut bitmap, width, height, line, padding, line_y, true);
        line_y += 16;
    }
    line_y += title_gap;
    for line in &body_lines {
        draw_text_line(&mut bitmap, width, height, line, padding, line_y, false);
        line_y += 16;
    }
    Ok((bitmap, width, height))
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::render_bubble;

    #[test]
    fn linux_bubble_returns_bgra_bitmap() {
        let (bitmap, width, height) = render_bubble("Title", "Hello Linux", 260).unwrap();
        assert_eq!(bitmap.len(), (width * height * 4) as usize);
        assert!(bitmap.as_chunks::<4>().0.contains(&[255, 255, 255, 255]));
        assert!(bitmap.as_chunks::<4>().0.contains(&[0, 0, 0, 255]));
    }

    #[test]
    fn linux_bubble_rejects_empty_text() {
        assert!(render_bubble("", "  ", 260).is_err());
    }
}
