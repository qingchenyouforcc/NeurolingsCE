//! 单只桌宠的完整运行时状态：位置、姿态、拖拽、交互与繁殖请求。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::behavior::Behavior;
use crate::broadcast::Interaction;
use crate::environment::{Border, DVec2, Environment};
use crate::math::{Rec, Vec2};
use crate::pose::Frame;

/// 繁殖请求：由 Breed/Transform 动作发起，运行时据此生成新桌宠。
#[derive(Debug, Clone, Default)]
pub struct BreedRequest {
    pub available: bool,
    pub transient: bool,
    pub looking_right: bool,
    pub anchor: Vec2,
    /// 目标桌宠模板名；为空表示繁殖同款。
    pub name: String,
    pub behavior: String,
}

pub struct State {
    pub breed_request: BreedRequest,
    pub bounds: Rec,
    pub anchor: Vec2,
    pub active_frame: Frame,
    pub env: Option<Rc<RefCell<Environment>>>,
    pub constants: HashMap<String, String>,
    pub interaction: Interaction,
    pub queued_behavior: String,
    pub active_sound: String,
    pub active_sound_changed: bool,
    pub looking_right: bool,
    pub dragging: bool,
    pub was_on_ie: bool,
    pub dead: bool,
    pub time: i64,
    pub can_breed: bool,
    pub next_subtick: i32,
    pub drag_with_local_cursor: bool,
    pub local_cursor: DVec2,
    stored_dcursor_data: Vec<Vec2>,
    stored_dcursor: DVec2,
    next_dcursor_roll: usize,
    pub active_ie_offset: Vec2,
    pub behavior: Option<Rc<Behavior>>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            breed_request: BreedRequest::default(),
            bounds: Rec::default(),
            anchor: Vec2::ZERO,
            active_frame: Frame::default(),
            env: None,
            constants: HashMap::new(),
            interaction: Interaction::default(),
            queued_behavior: String::new(),
            active_sound: String::new(),
            active_sound_changed: false,
            looking_right: false,
            dragging: false,
            was_on_ie: false,
            dead: false,
            time: 0,
            can_breed: true,
            next_subtick: 0,
            drag_with_local_cursor: false,
            local_cursor: DVec2::default(),
            stored_dcursor_data: Vec::new(),
            stored_dcursor: DVec2::default(),
            next_dcursor_roll: 0,
            active_ie_offset: Vec2::ZERO,
            behavior: None,
        }
    }
}

pub type SharedState = Rc<RefCell<State>>;

pub fn shared_state(state: State) -> SharedState {
    Rc::new(RefCell::new(state))
}

impl State {
    pub fn roll_dcursor(&mut self) {
        let roller_size = match &self.env {
            None => 2,
            Some(env) => env.borrow_mut().sanitized_subtick_count() as usize + 1,
        };
        if self.stored_dcursor_data.len() != roller_size {
            self.stored_dcursor = DVec2::default();
            self.stored_dcursor_data = vec![Vec2::ZERO; roller_size];
            self.next_dcursor_roll = 0;
        }

        let raw_cursor = self.get_raw_cursor();
        if self.dragging {
            self.stored_dcursor_data[self.next_dcursor_roll] =
                Vec2::new(raw_cursor.dx, raw_cursor.dy);
        } else {
            self.stored_dcursor_data[self.next_dcursor_roll] = Vec2::ZERO;
        }

        let mut stored = DVec2::with_delta(raw_cursor.x, raw_cursor.y, 0.0, 0.0);
        for vec in &self.stored_dcursor_data {
            stored.dx += vec.x;
            stored.dy += vec.y;
        }
        self.stored_dcursor = stored;
        self.next_dcursor_roll = (self.next_dcursor_roll + 1) % roller_size;
    }

    fn get_raw_cursor(&self) -> DVec2 {
        if self.dragging && self.drag_with_local_cursor {
            self.local_cursor
        } else {
            match &self.env {
                Some(env) => env.borrow().cursor,
                None => DVec2::default(),
            }
        }
    }

    pub fn get_cursor(&self) -> DVec2 {
        self.stored_dcursor
    }

    pub fn on_land(&self) -> bool {
        let Some(env) = &self.env else { return false };
        let env = env.borrow();
        env.floor.is_on(self.anchor)
            || env.ceiling.is_on(self.anchor)
            || env.work_area.is_on(self.anchor)
            || env.active_ie.is_on(self.anchor)
    }
}
