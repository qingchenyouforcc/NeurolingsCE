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
    pub pending_codex_bubble: Option<(String, String, std::time::Duration)>,
    /// 当前气泡的位图与尺寸（跟随移动时复用）。
    pub bubble_bitmap: Option<Vec<u8>>,
    pub bubble_size: (u32, u32),
}

impl Session {
    /// 按当前帧状态重绘窗口。
    pub fn render(&mut self) {
        let (frame, looking_right, anchor) = {
            let s = self.manager.state.borrow();
            (s.active_frame.clone(), s.looking_right, s.anchor)
        };
        let name = frame.get_name(looking_right).to_lowercase();
        if name.is_empty() {
            return;
        }
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
        let (width, height) = (bitmap.width, bitmap.height);
        let (anchor_x, anchor_y) = if mirrored {
            (width as f64 - frame.anchor.x, frame.anchor.y)
        } else {
            (frame.anchor.x, frame.anchor.y)
        };
        let top_left = Point::new(
            (anchor.x - anchor_x).round() as i32,
            (anchor.y - anchor_y).round() as i32,
        );
        self.window_top_left = top_left;
        self.frame_size = (width, height);
        if let Err(err) = self.window.update_frame(buffer, width, height, top_left) {
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
    let raw_name = name.strip_prefix('/').unwrap_or(name);
    let path = img_dir.join(raw_name);
    let img = match image::open(&path) {
        Ok(img) => img.to_rgba8(),
        Err(err) => {
            eprintln!("mascot {mascot_name}: missing frame {name}: {err}");
            return None;
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
    let data_id = templates
        .names_sorted()
        .iter()
        .position(|n| n == name)
        .unwrap_or(0) as i64;
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
        pending_codex_bubble: None,
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
