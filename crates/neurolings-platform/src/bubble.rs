//! 语气泡位图渲染（Windows GDI）：白底圆角矩形上绘制深色文本，
//! 返回带逐像素 alpha 的预乘 BGRA——内容 DC 提供颜色，掩码 DC 提供
//! 气泡轮廓。

#[cfg(windows)]
pub fn render_bubble(text: &str, max_width: u32) -> Result<(Vec<u8>, u32, u32), String> {
    use windows::Win32::Foundation::COLORREF;
    use windows::Win32::Graphics::Gdi::ReleaseDC;
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BLACK_BRUSH, CreateCompatibleDC, CreateDIBSection, CreateFontW,
        CreateSolidBrush, DIB_RGB_COLORS, DT_CALCRECT, DT_LEFT, DT_NOCLIP, DT_TOP, DT_WORDBREAK,
        DeleteDC, DeleteObject, DrawTextW, FillRect, GetDC, GetDIBits, GetStockObject, RoundRect,
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

        let make_dc = || -> Result<(windows::Win32::Graphics::Gdi::HDC, windows::Win32::Graphics::Gdi::HBITMAP, *mut u8), String> {
            let dc = CreateCompatibleDC(Some(screen_dc));
            let mut bmi = BITMAPINFO::default();
            bmi.bmiHeader.biSize = std::mem::size_of::<windows::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = width as i32;
            bmi.bmiHeader.biHeight = -200i32; // 自上而下，预留换行高度
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB.0;
            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let hbmp = CreateDIBSection(Some(dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
                .map_err(|e| e.to_string())?;
            let _ = SelectObject(dc, windows::Win32::Graphics::Gdi::HGDIOBJ(hbmp.0));
            Ok((dc, hbmp, bits as *mut u8))
        };

        let (content_dc, content_bmp, content_bits) = make_dc()?;
        let (mask_dc, mask_bmp, mask_bits) = make_dc()?;

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
        let _ = SelectObject(content_dc, windows::Win32::Graphics::Gdi::HGDIOBJ(font.0));
        let height_needed = DrawTextW(
            content_dc,
            &mut wide.clone(),
            &mut rect,
            DT_CALCRECT | DT_LEFT | DT_TOP | DT_WORDBREAK,
        );
        let text_height = height_needed.max(font_height) as u32;
        let height = (text_height + padding * 2).clamp(40, 200);

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
        let _ = SelectObject(mask_dc, windows::Win32::Graphics::Gdi::HGDIOBJ(pen.0));
        let _ = SelectObject(mask_dc, windows::Win32::Graphics::Gdi::HGDIOBJ(white.0));
        let _ = RoundRect(
            mask_dc,
            1,
            1,
            (width - 1) as i32,
            (height - 1) as i32,
            18,
            18,
        );

        // 回读两张位图（GetDIBits 返回自下而上的行）。
        let read_pixels = |dc, bits: *mut u8| -> Vec<u8> {
            let mut out = vec![0u8; (width * height * 4) as usize];
            let mut bmi = BITMAPINFO::default();
            bmi.bmiHeader.biSize =
                std::mem::size_of::<windows::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = width as i32;
            bmi.bmiHeader.biHeight = height as i32; // 自下而上回读
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB.0;
            let mut bmi_read = bmi;
            let _ = GetDIBits(
                dc,
                windows::Win32::Graphics::Gdi::HBITMAP(bits as *mut _),
                0,
                height,
                Some(out.as_mut_ptr() as *mut _),
                &mut bmi_read,
                DIB_RGB_COLORS,
            );
            out
        };
        let _ = (content_bits, mask_bits);
        let content_px = read_pixels(content_dc, content_bits);
        let mask_px = read_pixels(mask_dc, mask_bits);

        // 合成：alpha 取掩码亮度、颜色取内容位图，再做预乘。
        let mut out = vec![0u8; (width * height * 4) as usize];
        for y in 0..height {
            // GetDIBits 返回自下而上，翻转为自上而下。
            let src_row = (height - 1 - y) as usize;
            for x in 0..width {
                let i = (src_row * width as usize + x as usize) * 4;
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

        let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(font.0));
        let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(white.0));
        let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(content_bmp.0));
        let _ = DeleteObject(windows::Win32::Graphics::Gdi::HGDIOBJ(mask_bmp.0));
        let _ = DeleteDC(content_dc);
        let _ = DeleteDC(mask_dc);
        let _ = ReleaseDC(None, screen_dc);

        Ok((out, width, height))
    }
}

#[cfg(not(windows))]
pub fn render_bubble(_text: &str, _max_width: u32) -> Result<(Vec<u8>, u32, u32), String> {
    Err("speech bubbles are not implemented on this platform yet".into())
}
