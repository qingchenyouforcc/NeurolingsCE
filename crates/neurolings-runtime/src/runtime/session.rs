//! 单只桌宠的运行时会话：引擎管理器、窗口、帧缓存与外围状态。

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use neurolings_engine::environment::Environment;
use neurolings_engine::mascot::{Factory, Initializer, Manager};
use neurolings_engine::math::Vec2;
use neurolings_platform::{MascotWindow, Point};

use crate::fallthrough::FallThroughTracker;
use crate::runtime::interaction::Gesture;
use crate::runtime::sounds::SoundPlayer;
use crate::templates::TemplateStore;

/// 引擎子帧数与帧间隔：每 40ms 一个整帧，拆成 4 个子帧推进。
pub const SUBTICK_COUNT: i32 = 4;
pub const TICK_INTERVAL_MS: u64 = 40 / SUBTICK_COUNT as u64;

/// 解码并预乘后的帧位图（BGRA）。
pub struct FrameBitmap {
    pub width: u32,
    pub height: u32,
    pub premul_bgra: Vec<u8>,
    pub mirrored: Option<Vec<u8>>,
}

pub struct Session {
    pub id: u64,
    pub data_id: i64,
    pub name: String,
    pub label: Option<i64>,
    pub manager: Manager,
    pub window: Box<dyn MascotWindow>,
    /// 模板解压目录下的 img 路径。
    pub(crate) img_dir: PathBuf,
    /// 模板根目录（气泡文案、音效查找用）。
    pub pack_dir: PathBuf,
    pub(crate) frames: HashMap<String, FrameBitmap>,
    pub dragging: bool,
    pub fall_tracker: FallThroughTracker,
    pub gesture: Gesture,
    pub sounds: Option<SoundPlayer>,
    pub paused: bool,
    pub dead: bool,
    /// 当前帧窗口左上角的屏幕坐标（气泡定位用）。
    pub window_top_left: Point,
    /// 当前帧位图尺寸。
    pub frame_size: (u32, u32),
    /// 沙盒（窗口）模式下的标记。
    pub windowed: bool,
    /// 气泡窗口（懒创建，复用）。
    pub bubble_window: Option<Box<dyn MascotWindow>>,
    pub bubble_until: Instant,
    /// 待显示普通气泡文本。
    pub pending_bubble: Option<String>,
    /// 待显示 Codex 气泡（标题, 正文, 时长）。
    /// Codex 气泡队列（上限 8，满时丢最旧，对齐原版 SpeechBubble 队列）。
    pub codex_bubble_queue: std::collections::VecDeque<(String, String, std::time::Duration)>,
    /// 当前显示的气泡是否为 Codex 气泡（点击时跳转 Codex 页）。
    pub bubble_is_codex: bool,
    /// 当前气泡的位图与尺寸（跟随移动时复用）。
    pub bubble_bitmap: Option<Vec<u8>>,
    pub bubble_size: (u32, u32),
}

/// 把预乘 BGRA 按最近邻缩放到目标尺寸。
pub fn scale_premul_bgra(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    let mut out = vec![0u8; dw as usize * dh as usize * 4];
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 {
        return out;
    }
    for y in 0..dh {
        let sy = y * sh / dh;
        for x in 0..dw {
            let sx = x * sw / dw;
            let si = ((sy * sw + sx) * 4) as usize;
            let di = ((y * dw + x) * 4) as usize;
            out[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    out
}

/// 把预乘 BGRA 在窗口内平移 (off_x, off_y)：目标像素 (x,y) 取源像素
/// (x-off_x, y-off_y)，越界部分透明。用于窗口被钳制在屏幕内时保持
/// 内容的视觉位置不变（采用负 drawOrigin 绘制的偏移规则）。
fn offset_premul_bgra(src: &[u8], w: u32, h: u32, off_x: i32, off_y: i32) -> Vec<u8> {
    let mut out = vec![0u8; (w * h * 4) as usize];
    let (w, h) = (w as i32, h as i32);
    for dy in 0..h {
        let sy = dy - off_y;
        if !(0..h).contains(&sy) {
            continue;
        }
        let dx0 = off_x.max(0);
        let dx1 = (off_x + w).min(w);
        for dx in dx0..dx1 {
            let sx = dx - off_x;
            let si = ((sy * w + sx) * 4) as usize;
            let di = ((dy * w + dx) * 4) as usize;
            out[di..di + 4].copy_from_slice(&src[si..si + 4]);
        }
    }
    out
}

impl Session {
    /// 按当前帧状态重绘窗口。
    pub fn render(&mut self) {
        let (frame, looking_right, anchor, env_scale, screen) = {
            let s = self.manager.state.borrow();
            let scale = s
                .env
                .as_ref()
                .map(|env| env.borrow().get_scale())
                .filter(|scale| scale.is_finite() && *scale > 0.0)
                .unwrap_or(1.0)
                .clamp(0.05, 20.0);
            let screen = s.env.as_ref().map(|env| env.borrow().screen);
            (
                s.active_frame.clone(),
                s.looking_right,
                s.anchor,
                scale,
                screen,
            )
        };
        let raw_name = frame.get_name(looking_right).to_lowercase();
        // 空帧名回退占位图，避免窗口停在上一帧或消失。
        let name = if raw_name.is_empty() {
            "__missing__.png".to_string()
        } else {
            raw_name
        };
        let Some(bitmap) = load_frame(&mut self.frames, &self.img_dir, &name, &self.name) else {
            return;
        };
        // 没有右朝向专用帧时水平镜像。
        let mirrored = looking_right && frame.right_name.is_empty();
        let buffer = if mirrored {
            bitmap.mirrored.as_ref().unwrap_or(&bitmap.premul_bgra)
        } else {
            &bitmap.premul_bgra
        };
        let (src_w, src_h) = (bitmap.width, bitmap.height);
        // 窗口尺寸 = 原图像素 / env_scale，遵循 updateOffsets 的尺寸计算规则。
        let dest_w = ((src_w as f64) / env_scale).round().max(1.0) as u32;
        let dest_h = ((src_h as f64) / env_scale).round().max(1.0) as u32;
        let drawn = if dest_w == src_w && dest_h == src_h {
            None
        } else {
            Some(scale_premul_bgra(buffer, src_w, src_h, dest_w, dest_h))
        };
        let pixels = drawn.as_deref().unwrap_or(buffer);
        let (anchor_x, anchor_y) = if mirrored {
            (
                (src_w as f64 - frame.anchor.x) / env_scale,
                frame.anchor.y / env_scale,
            )
        } else {
            (frame.anchor.x / env_scale, frame.anchor.y / env_scale)
        };
        // 窗口钳进桌宠所在屏幕，越界方向用绘制偏移补偿（参考实现
        // MascotWidgetRendering.cc 的 drawOffset 语义：窗口尺寸不变，
        // 内容平移、越界部分裁剪）。
        let mut win_x = (anchor.x - anchor_x).round() as i32;
        let mut win_y = (anchor.y - anchor_y).round() as i32;
        let mut off_x = 0i32;
        let mut off_y = 0i32;
        if let Some(screen) = screen.filter(|s| s.visible()) {
            let scr_w = screen.width().round() as i32;
            let scr_h = screen.height().round() as i32;
            let (scr_l, scr_t) = (screen.left.round() as i32, screen.top.round() as i32);
            let mut rx = win_x - scr_l;
            let mut ry = win_y - scr_t;
            if rx < 0 {
                off_x = rx;
                rx = 0;
            } else if rx + dest_w as i32 > scr_w {
                off_x = rx + dest_w as i32 - scr_w;
                rx = scr_w - dest_w as i32;
            }
            if ry < 0 {
                off_y = ry;
                ry = 0;
            } else if ry + dest_h as i32 > scr_h {
                off_y = ry + dest_h as i32 - scr_h;
                ry = scr_h - dest_h as i32;
            }
            win_x = rx + scr_l;
            win_y = ry + scr_t;
        }
        let top_left = Point::new(win_x, win_y);
        self.window_top_left = top_left;
        self.frame_size = (dest_w, dest_h);
        let shifted;
        let pixels = if off_x != 0 || off_y != 0 {
            shifted = offset_premul_bgra(pixels, dest_w, dest_h, off_x, off_y);
            &shifted[..]
        } else {
            pixels
        };
        if let Err(err) = self.window.update_frame(pixels, dest_w, dest_h, top_left) {
            eprintln!("mascot {}: render failed: {err}", self.name);
        }
    }

    /// 沙盒合成用：取缓存帧位图（不存在时从模板图像解码）。
    pub fn sandbox_frame(&mut self, name: &str) -> Option<&FrameBitmap> {
        load_frame(&mut self.frames, &self.img_dir, name, &self.name)
    }
}

fn load_frame<'a>(
    frames: &'a mut HashMap<String, FrameBitmap>,
    img_dir: &std::path::Path,
    name: &str,
    mascot_name: &str,
) -> Option<&'a FrameBitmap> {
    if frames.contains_key(name) {
        return frames.get(name);
    }
    // 循环去掉所有前导 '/'，再强制 .png 结尾且路径必须落在 img_dir 内
    // （既定契约：循环 strip 前导 '/' + SafePath::safeChildPath + 强制 .png）。
    let raw_name = name.trim_start_matches('/');
    let safe_path = if raw_name.to_ascii_lowercase().ends_with(".png") {
        neurolings_pack::safe_child_path(img_dir, raw_name)
    } else {
        None
    };
    // 虚拟模板 @ 的帧取内嵌资源；普通模板读包目录。
    let decoded = safe_path.and_then(|path| {
        if crate::templates::is_default_template(mascot_name) {
            crate::templates::DEFAULT_MASCOT
                .get_file(format!("img/{raw_name}"))
                .and_then(|f| image::load_from_memory(f.contents()).ok())
        } else {
            image::open(path).ok()
        }
    });
    let img = match decoded {
        Some(img) => img.to_rgba8(),
        None => {
            crate::log::warn(
                "mascot",
                &format!("mascot {mascot_name}: missing frame {name}"),
            );
            // 缺失帧用透明占位，不让窗口卡在上一帧。
            frames.insert(
                name.to_string(),
                FrameBitmap {
                    width: 1,
                    height: 1,
                    premul_bgra: vec![0, 0, 0, 0],
                    mirrored: Some(vec![0, 0, 0, 0]),
                },
            );
            return frames.get(name);
        }
    };
    let (width, height) = img.dimensions();
    let premul_bgra = premultiply_bgra(&img);
    let mirrored_img = image::imageops::flip_horizontal(&img);
    let mirrored = Some(premultiply_bgra(&mirrored_img));
    let bitmap = FrameBitmap {
        width,
        height,
        premul_bgra,
        mirrored,
    };
    frames.insert(name.to_string(), bitmap);
    frames.get(name)
}

fn premultiply_bgra(img: &image::RgbaImage) -> Vec<u8> {
    let mut out = Vec::with_capacity((img.width() * img.height() * 4) as usize);
    for pixel in img.pixels() {
        let [r, g, b, a] = pixel.0;
        let af = a as u32;
        out.push(((b as u32 * af) / 255) as u8);
        out.push(((g as u32 * af) / 255) as u8);
        out.push(((r as u32 * af) / 255) as u8);
        out.push(a);
    }
    out
}

/// 创建桌宠会话：生成引擎管理器、绑定环境、创建窗口并预渲染首帧。
///
/// 锚点为 None 时在屏幕内随机取点；空行为名表示由行为管理器自动选择初始行为。
#[allow(clippy::too_many_arguments)]
pub fn create_session(
    sessions: &mut Vec<Session>,
    factory: &Factory,
    backend: &mut Option<&mut Box<dyn neurolings_platform::MascotBackend>>,
    env: &Rc<RefCell<Environment>>,
    templates: &TemplateStore,
    next_id: &mut u64,
    name: &str,
    anchor: Option<Vec2>,
    behavior: &str,
) -> Result<u64, String> {
    // data_id 取自稳定注册表（加载顺序递增、运行期间不变），不按排序下标。
    let data_id = crate::services::template_data_id(templates, name);
    let init = Initializer::new(anchor.unwrap_or(Vec2::ZERO), behavior, false);
    let product = factory.spawn(name, init).map_err(|e| e.to_string())?;
    product.manager.state.borrow_mut().env = Some(env.clone());
    if anchor.is_none() {
        product.manager.reset_position();
    }
    let id = *next_id;
    *next_id += 1;
    let window: Box<dyn MascotWindow> = match backend.as_deref_mut() {
        Some(b) => b.create_window(id).map_err(|e| e.to_string())?,
        None => Box::new(crate::headless::HeadlessWindow),
    };
    let pack_dir = templates.pack_dir(name).unwrap_or_default();
    let img_dir = pack_dir.join("img");
    let sound_dir = pack_dir.join("sound");
    let mut session = Session {
        id,
        data_id,
        name: name.to_string(),
        label: None,
        manager: product.manager,
        window,
        img_dir,
        pack_dir,
        frames: HashMap::new(),
        dragging: false,
        fall_tracker: FallThroughTracker::new(),
        gesture: Gesture::default(),
        sounds: SoundPlayer::new(&sound_dir),
        paused: false,
        dead: false,
        window_top_left: Point::new(0, 0),
        frame_size: (0, 0),
        windowed: false,
        bubble_window: None,
        bubble_until: Instant::now(),
        pending_bubble: None,
        codex_bubble_queue: std::collections::VecDeque::new(),
        bubble_is_codex: false,
        bubble_bitmap: None,
        bubble_size: (0, 0),
    };
    // 预渲染首帧：没有有效帧的模板视为损坏，直接拒绝。
    {
        let needs_tick = {
            let s = session.manager.state.borrow();
            s.active_frame.get_name(s.looking_right).is_empty()
        };
        if needs_tick {
            session.manager.tick().map_err(|e| e.to_string())?;
        }
    }
    sessions.push(session);
    env.borrow_mut().mascot_count = sessions.len() as i64;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::{FrameBitmap, load_frame, offset_premul_bgra, scale_premul_bgra};
    use std::collections::HashMap;

    #[test]
    fn scale_premul_bgra_identity() {
        let src = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let out = scale_premul_bgra(&src, 2, 1, 2, 1);
        assert_eq!(out, src);
    }

    #[test]
    fn offset_premul_bgra_shifts_and_clips() {
        // 2x1 位图：像素 A=(1,2,3,4)、B=(5,6,7,8)。
        let src = [1u8, 2, 3, 4, 5, 6, 7, 8];
        // 负偏移（窗口被钳到左边缘）：左半裁剪，只剩 B。
        let out = offset_premul_bgra(&src, 2, 1, -1, 0);
        assert_eq!(out, [5, 6, 7, 8, 0, 0, 0, 0]);
        // 正偏移（窗口被钳到右边缘）：内容右移，A 落在 (1,0)。
        let out = offset_premul_bgra(&src, 2, 1, 1, 0);
        assert_eq!(out, [0, 0, 0, 0, 1, 2, 3, 4]);
        // y 方向越界：全透明。
        let out = offset_premul_bgra(&src, 2, 1, 0, 5);
        assert_eq!(out, [0; 8]);
    }

    /// 生成 1x1 红色 PNG 字节。
    fn test_png_bytes() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        let mut buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf, image::ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    /// 是否为缺失帧的 1x1 全透明占位图。
    fn is_placeholder(bitmap: &FrameBitmap) -> bool {
        bitmap.width == 1 && bitmap.height == 1 && bitmap.premul_bgra == [0, 0, 0, 0]
    }

    fn load<'a>(
        frames: &'a mut HashMap<String, FrameBitmap>,
        img_dir: &std::path::Path,
        name: &str,
    ) -> &'a FrameBitmap {
        load_frame(frames, img_dir, name, "testmascot").expect("missing frame fallback")
    }

    #[test]
    fn load_frame_rejects_path_traversal() {
        let tempdir = tempfile::tempdir().unwrap();
        let img_dir = tempdir.path().join("img");
        std::fs::create_dir(&img_dir).unwrap();
        // 包外的文件即便存在也不得被读到。
        std::fs::write(tempdir.path().join("evil.png"), test_png_bytes()).unwrap();
        let mut frames = HashMap::new();
        assert!(is_placeholder(load(&mut frames, &img_dir, "../evil.png")));
        assert!(is_placeholder(load(&mut frames, &img_dir, "//../evil.png")));
        assert!(is_placeholder(load(&mut frames, &img_dir, "..\\evil.png")));
    }

    #[test]
    fn load_frame_rejects_absolute_paths() {
        let tempdir = tempfile::tempdir().unwrap();
        let img_dir = tempdir.path().join("img");
        std::fs::create_dir(&img_dir).unwrap();
        let mut frames = HashMap::new();
        assert!(is_placeholder(load(
            &mut frames,
            &img_dir,
            "/etc/passwd.png"
        )));
        assert!(is_placeholder(load(
            &mut frames,
            &img_dir,
            "C:/Windows/x.png"
        )));
    }

    #[test]
    fn load_frame_rejects_non_png_names() {
        let tempdir = tempfile::tempdir().unwrap();
        let img_dir = tempdir.path().join("img");
        std::fs::create_dir(&img_dir).unwrap();
        // 文件存在但不是 .png 结尾，同样按缺失帧处理。
        std::fs::write(img_dir.join("note.txt"), b"hello").unwrap();
        let mut frames = HashMap::new();
        assert!(is_placeholder(load(&mut frames, &img_dir, "note.txt")));
        assert!(is_placeholder(load(&mut frames, &img_dir, "shime1")));
    }

    #[test]
    fn load_frame_accepts_valid_png_inside_img_dir() {
        let tempdir = tempfile::tempdir().unwrap();
        let img_dir = tempdir.path().join("img");
        std::fs::create_dir(&img_dir).unwrap();
        std::fs::write(img_dir.join("shime1.png"), test_png_bytes()).unwrap();
        let mut frames = HashMap::new();
        let bitmap = load(&mut frames, &img_dir, "/shime1.png");
        assert!(!is_placeholder(bitmap));
        assert_eq!((bitmap.width, bitmap.height), (1, 1));
        // 红色不透明像素的 premul BGRA。
        assert_eq!(bitmap.premul_bgra, [0, 0, 255, 255]);
    }
}
