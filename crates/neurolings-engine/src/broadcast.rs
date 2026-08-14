//! 桌宠间广播：动作以"Affordance"广播会合点，ScanMove 扫描并连接，
//! 双方靠近后各自进入交互行为。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use crate::math::Vec2;

/// 会合扫描的最大水平距离：按 128px 标准画布的两倍取 256，
/// 足以靠近同伴，又不会隔着整个桌面锁定目标。
const MAX_SCAN_DISTANCE: f64 = 256.0;

#[derive(Debug, Default)]
pub struct ServerState {
    finalized: bool,
    available: bool,
    met_up: bool,
    ongoing: Option<Rc<RefCell<bool>>>,
    pub anchor: Vec2,
    pub client_behavior: String,
    pub server_behavior: String,
}

impl ServerState {
    fn new(anchor: Vec2) -> Self {
        Self {
            available: true,
            anchor,
            ..Default::default()
        }
    }
    pub fn did_meet_up(&self) -> bool {
        self.met_up
    }
    pub fn notify_arrival(&mut self) {
        self.met_up = true;
        self.finalize();
    }
    pub fn active(&self) -> bool {
        !self.finalized
    }
    pub fn available(&self) -> bool {
        self.active() && self.available
    }
    pub fn finalize(&mut self) {
        self.finalized = true;
    }
    pub fn set_available(&mut self, available: bool) {
        self.available = available;
    }
    pub fn ongoing_pt(&mut self) -> Rc<RefCell<bool>> {
        self.ongoing
            .get_or_insert_with(|| Rc::new(RefCell::new(true)))
            .clone()
    }
}

type SharedServerState = Rc<RefCell<ServerState>>;

/// 一次进行中的桌宠交互；默认值表示无交互。
#[derive(Debug, Clone, Default)]
pub struct Interaction {
    ongoing: Option<Rc<RefCell<bool>>>,
    behavior: String,
    pub started: bool,
}

impl Interaction {
    pub fn new(ongoing: Rc<RefCell<bool>>, behavior: String) -> Self {
        Self {
            ongoing: Some(ongoing),
            behavior,
            started: false,
        }
    }
    pub fn behavior(&self) -> &str {
        &self.behavior
    }
    pub fn ongoing(&self) -> bool {
        self.ongoing.as_ref().is_some_and(|f| *f.borrow())
    }
    pub fn available(&self) -> bool {
        self.ongoing.is_some()
    }
    pub fn finalize(&mut self) {
        if let Some(flag) = self.ongoing.take() {
            *flag.borrow_mut() = false;
        }
        self.behavior.clear();
        self.started = false;
    }
}

#[derive(Debug, Clone, Default)]
pub struct Server {
    state: Option<SharedServerState>,
}

impl Server {
    pub fn new(anchor: Vec2) -> Self {
        Self {
            state: Some(Rc::new(RefCell::new(ServerState::new(anchor)))),
        }
    }
    pub fn active(&self) -> bool {
        self.state.as_ref().is_some_and(|s| s.borrow().active())
    }
    pub fn available(&self) -> bool {
        self.state.as_ref().is_some_and(|s| {
            let s = s.borrow();
            !s.did_meet_up() && s.available()
        })
    }
    pub fn did_meet_up(&self) -> bool {
        self.state
            .as_ref()
            .is_some_and(|s| s.borrow().did_meet_up())
    }
    pub fn update_anchor(&mut self, anchor: Vec2) {
        let Some(state) = &self.state else { return };
        if !state.borrow().active() {
            // 广播已结束，忽略迟到的锚点更新。
            return;
        }
        state.borrow_mut().anchor = anchor;
    }
    pub fn get_anchor(&self) -> Vec2 {
        self.state
            .as_ref()
            .map_or(Vec2::ZERO, |s| s.borrow().anchor)
    }
    pub fn finalize(&mut self) {
        if let Some(state) = self.state.take() {
            state.borrow_mut().finalize();
        }
    }
    pub fn connect(&mut self, client_behavior: &str, server_behavior: &str) -> Option<Client> {
        if !self.available() {
            return None;
        }
        let state = self.state.as_ref()?;
        {
            let mut s = state.borrow_mut();
            s.client_behavior = client_behavior.to_string();
            s.server_behavior = server_behavior.to_string();
            s.set_available(false);
        }
        Some(Client {
            server: Some(Rc::downgrade(state)),
        })
    }
    pub fn get_interaction(&self) -> Option<Interaction> {
        if !self.did_meet_up() {
            return None;
        }
        let state = self.state.as_ref()?;
        let mut s = state.borrow_mut();
        Some(Interaction::new(s.ongoing_pt(), s.server_behavior.clone()))
    }
}

#[derive(Debug, Default)]
pub struct Client {
    server: Option<Weak<RefCell<ServerState>>>,
}

impl Client {
    fn shared(&self) -> Option<SharedServerState> {
        self.server.as_ref().and_then(Weak::upgrade)
    }
    pub fn finalize(&mut self) {
        if let Some(state) = self.shared() {
            state.borrow_mut().set_available(true);
        }
        self.server = None;
    }
    pub fn connected(&self) -> bool {
        self.shared().is_some_and(|s| s.borrow().active())
    }
    pub fn notify_arrival(&self) {
        if let Some(state) = self.shared() {
            state.borrow_mut().notify_arrival();
        }
    }
    pub fn did_meet_up(&self) -> bool {
        self.shared().is_some_and(|s| s.borrow().did_meet_up())
    }
    pub fn get_target(&self) -> Option<Vec2> {
        self.shared().map(|s| s.borrow().anchor)
    }
    pub fn get_interaction(&self) -> Option<Interaction> {
        if !self.did_meet_up() {
            return None;
        }
        let state = self.shared()?;
        let mut s = state.borrow_mut();
        Some(Interaction::new(s.ongoing_pt(), s.client_behavior.clone()))
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        self.finalize();
    }
}

#[derive(Default)]
pub struct BroadcastManager {
    servers: HashMap<String, Vec<SharedServerState>>,
}

impl BroadcastManager {
    pub fn start_broadcast(&mut self, affordance: &str, anchor: Vec2) -> Server {
        let server = Server::new(anchor);
        self.servers
            .entry(affordance.to_string())
            .or_default()
            .push(server.state.as_ref().unwrap().clone());
        server
    }

    pub fn try_connect(
        &mut self,
        anchor: Vec2,
        affordance: &str,
        client_behavior: &str,
        server_behavior: &str,
    ) -> Option<Client> {
        let servers = self.servers.entry(affordance.to_string()).or_default();
        servers.retain(|s| s.borrow().active());

        let mut nearest: Option<(SharedServerState, f64)> = None;
        for candidate in servers.iter() {
            let (target, available) = {
                let c = candidate.borrow();
                (c.anchor, c.available())
            };
            let distance = (anchor.x - target.x).abs();
            if (anchor.y - target.y).abs() > 1.0 || distance > MAX_SCAN_DISTANCE || !available {
                continue;
            }
            match &nearest {
                Some((_, d)) if *d <= distance => {}
                _ => nearest = Some((candidate.clone(), distance)),
            }
        }
        let (state, _) = nearest?;
        let mut server = Server { state: Some(state) };
        server.connect(client_behavior, server_behavior)
    }
}
