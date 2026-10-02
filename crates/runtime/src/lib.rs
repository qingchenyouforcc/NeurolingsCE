//! 模板、会话和 tick 生命周期编排入口。
//!
//! 运行时只管理内存中的 mascot 会话和固定频率时钟，不创建平台窗口，也不负责
//! 网络或持久化。上层传输和界面可以通过快照读取状态，再在同一线程调用变更方法。

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

pub use api::{Anchor, Label};
use thiserror::Error;

/// 每个主 tick 的固定间隔。
pub const TICK_INTERVAL: Duration = Duration::from_millis(40);

/// 每个主 tick 拆分出的引擎 subtick 数量。
pub const SUBTICK_COUNT: u32 = 4;

/// mascot 会话生命周期错误。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuntimeError {
    /// 名称为空时无法创建会话。
    #[error("mascot name must not be empty")]
    EmptyName,
    /// 模板数据 ID 必须是非负数。
    #[error("mascot data id must be non-negative: {0}")]
    InvalidDataId(i32),
    /// 锚点中存在非有限坐标。
    #[error("mascot anchor coordinates must be finite")]
    InvalidAnchor,
    /// 目标 mascot 不存在。
    #[error("mascot not found: {0}")]
    MascotNotFound(i32),
    /// CLI label 已被另一个 mascot 占用。
    #[error("CLI label is already in use: {0:?}")]
    LabelInUse(Label),
    /// mascot 已经绑定了另一个 CLI label。
    #[error("mascot {mascot_id} already has a different CLI label")]
    LabelConflict { mascot_id: i32 },
    /// 可分配的 mascot ID 已耗尽。
    #[error("mascot id space is exhausted")]
    IdExhausted,
    /// 可分配的 CLI label 已耗尽。
    #[error("CLI label space is exhausted")]
    LabelExhausted,
}

/// 创建 mascot 会话所需的字段。
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnRequest {
    /// 模板名称。
    pub name: String,
    /// 模板数据 ID。
    pub data_id: i32,
    /// 初始屏幕锚点。
    pub anchor: Anchor,
    /// 可选的 CLI label。
    pub label: Option<Label>,
    /// 可选的当前行为名称。
    pub active_behavior: Option<String>,
}

impl SpawnRequest {
    /// 创建一个使用原点、无 label 和无行为的召唤请求。
    pub fn new(name: impl Into<String>, data_id: i32) -> Self {
        Self {
            name: name.into(),
            data_id,
            anchor: Anchor { x: 0.0, y: 0.0 },
            label: None,
            active_behavior: None,
        }
    }

    /// 设置初始锚点。
    pub fn with_anchor(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// 设置 CLI label。
    pub fn with_label(mut self, label: Label) -> Self {
        self.label = Some(label);
        self
    }

    /// 设置当前行为名称。
    pub fn with_active_behavior(mut self, behavior: impl Into<String>) -> Self {
        self.active_behavior = Some(behavior.into());
        self
    }
}

/// 当前一个 mascot 会话的不可变值对象。
///
/// 运行时内部保存该值的可变集合；调用 [`Runtime::snapshot`] 后得到的是独立的
/// 克隆，因此调用方不能通过快照改变运行时状态。
#[derive(Clone, Debug, PartialEq)]
pub struct MascotSession {
    id: i32,
    name: String,
    data_id: i32,
    anchor: Anchor,
    label: Option<Label>,
    active_behavior: Option<String>,
}

impl MascotSession {
    /// 返回运行时会话 ID。
    pub fn id(&self) -> i32 {
        self.id
    }

    /// 返回模板名称。
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回模板数据 ID。
    pub fn data_id(&self) -> i32 {
        self.data_id
    }

    /// 返回当前锚点。
    pub fn anchor(&self) -> Anchor {
        self.anchor
    }

    /// 返回当前 CLI label。
    pub fn label(&self) -> Option<Label> {
        self.label
    }

    /// 返回当前行为名称。
    pub fn active_behavior(&self) -> Option<&str> {
        self.active_behavior.as_deref()
    }
}

/// 固定频率时钟推进结果。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TickReport {
    elapsed: Duration,
    ticks: u64,
    subticks: u64,
}

impl TickReport {
    /// 返回这次调用观察到的时间间隔。
    pub fn elapsed(self) -> Duration {
        self.elapsed
    }

    /// 返回这次调用实际执行的主 tick 数量。
    pub fn ticks(self) -> u64 {
        self.ticks
    }

    /// 返回这次调用实际执行的 subtick 数量。
    pub fn subticks(self) -> u64 {
        self.subticks
    }

    /// 判断这次调用是否至少执行了一个主 tick。
    pub fn did_tick(self) -> bool {
        self.ticks != 0
    }
}

/// 运行时的不可变状态快照。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuntimeSnapshot {
    sessions: Vec<MascotSession>,
    tick_count: u64,
    subtick_count: u64,
}

impl RuntimeSnapshot {
    /// 返回快照中的 mascot 会话，顺序与会话 ID 排序一致。
    pub fn sessions(&self) -> &[MascotSession] {
        &self.sessions
    }

    /// 返回 [`sessions`](Self::sessions) 的协议层别名。
    pub fn mascots(&self) -> &[MascotSession] {
        self.sessions()
    }

    /// 返回已经执行的主 tick 总数。
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// 返回已经执行的 subtick 总数。
    pub fn subtick_count(&self) -> u64 {
        self.subtick_count
    }

    /// 返回当前会话数量。
    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    /// 判断当前是否没有会话。
    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

/// 单线程拥有的 mascot 运行时。
///
/// 所有状态修改都由调用方在同一线程串行执行；本类型不启动线程，也不隐式执行
/// 异步任务。固定时钟可以使用 [`Runtime::tick_at`] 注入 `Instant`，也可以使用
/// [`Runtime::advance`] 在测试或上层调度器中注入经过的时长。
#[derive(Debug)]
pub struct Runtime {
    sessions: BTreeMap<i32, MascotSession>,
    next_id: i32,
    labels: BTreeMap<Label, i32>,
    next_label: u32,
    accumulator: Duration,
    last_tick: Option<Instant>,
    tick_count: u64,
    subtick_count: u64,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    /// 创建空运行时；首个 mascot 和模板 ID 从 0 开始分配。
    pub fn new() -> Self {
        Self {
            sessions: BTreeMap::new(),
            next_id: 0,
            labels: BTreeMap::new(),
            next_label: 0,
            accumulator: Duration::ZERO,
            last_tick: None,
            tick_count: 0,
            subtick_count: 0,
        }
    }

    /// 创建一个 mascot 会话并返回其不可变副本。
    pub fn spawn(&mut self, request: SpawnRequest) -> Result<MascotSession, RuntimeError> {
        self.validate_spawn_request(&request)?;

        if let Some(label) = request.label
            && self.labels.contains_key(&label)
        {
            return Err(RuntimeError::LabelInUse(label));
        }

        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(RuntimeError::IdExhausted)?;
        let session = MascotSession {
            id,
            name: request.name,
            data_id: request.data_id,
            anchor: request.anchor,
            label: request.label,
            active_behavior: request.active_behavior,
        };
        if let Some(label) = session.label {
            self.labels.insert(label, id);
        }
        self.sessions.insert(id, session.clone());
        Ok(session)
    }

    /// 以名称、数据 ID 和锚点快捷创建 mascot 会话。
    pub fn spawn_named(
        &mut self,
        name: impl Into<String>,
        data_id: i32,
        anchor: Anchor,
    ) -> Result<MascotSession, RuntimeError> {
        self.spawn(SpawnRequest::new(name, data_id).with_anchor(anchor))
    }

    /// 返回当前会话的独立副本，按会话 ID 排序。
    pub fn list(&self) -> Vec<MascotSession> {
        self.sessions.values().cloned().collect()
    }

    /// 返回 [`list`](Self::list) 的协议层别名。
    pub fn list_mascots(&self) -> Vec<MascotSession> {
        self.list()
    }

    /// 返回指定 ID 的会话副本。
    pub fn get(&self, mascot_id: i32) -> Result<MascotSession, RuntimeError> {
        self.sessions
            .get(&mascot_id)
            .cloned()
            .ok_or(RuntimeError::MascotNotFound(mascot_id))
    }

    /// 删除一个 mascot 会话并返回被删除的副本。
    pub fn dismiss(&mut self, mascot_id: i32) -> Result<MascotSession, RuntimeError> {
        let session = self
            .sessions
            .remove(&mascot_id)
            .ok_or(RuntimeError::MascotNotFound(mascot_id))?;
        if let Some(label) = session.label {
            self.labels.remove(&label);
        }
        Ok(session)
    }

    /// 删除全部会话并返回删除数量。
    pub fn dismiss_all(&mut self) -> usize {
        let count = self.sessions.len();
        self.sessions.clear();
        self.labels.clear();
        count
    }

    /// 删除名称完全匹配的会话并返回删除数量。
    pub fn dismiss_all_named(&mut self, name: &str) -> usize {
        let ids: Vec<i32> = self
            .sessions
            .values()
            .filter(|session| session.name == name)
            .map(MascotSession::id)
            .collect();
        let count = ids.len();
        for id in ids {
            let _ = self.dismiss(id);
        }
        count
    }

    /// 为 mascot 注册 CLI label；未提供首选值时分配最小的空闲 label。
    pub fn register_label(
        &mut self,
        mascot_id: i32,
        preferred: Option<Label>,
    ) -> Result<Label, RuntimeError> {
        let session = self
            .sessions
            .get(&mascot_id)
            .ok_or(RuntimeError::MascotNotFound(mascot_id))?;
        if let Some(existing) = session.label {
            return match preferred {
                None => Ok(existing),
                Some(label) if label == existing => Ok(existing),
                Some(_) => Err(RuntimeError::LabelConflict { mascot_id }),
            };
        }

        let label = match preferred {
            Some(label) => label,
            None => self.next_available_label()?,
        };
        if self.labels.contains_key(&label) {
            return Err(RuntimeError::LabelInUse(label));
        }
        let next_label = if label.value() >= self.next_label {
            Some(
                label
                    .value()
                    .checked_add(1)
                    .ok_or(RuntimeError::LabelExhausted)?,
            )
        } else {
            None
        };
        self.labels.insert(label, mascot_id);
        if let Some(session) = self.sessions.get_mut(&mascot_id) {
            session.label = Some(label);
        }
        if let Some(next_label) = next_label {
            self.next_label = next_label;
        }
        Ok(label)
    }

    /// 清除 mascot 当前的 CLI label；不存在的 mascot 返回错误。
    pub fn clear_label(&mut self, mascot_id: i32) -> Result<(), RuntimeError> {
        let session = self
            .sessions
            .get_mut(&mascot_id)
            .ok_or(RuntimeError::MascotNotFound(mascot_id))?;
        if let Some(label) = session.label.take() {
            self.labels.remove(&label);
        }
        Ok(())
    }

    /// 返回一个 mascot 的当前 label。
    pub fn label_for(&self, mascot_id: i32) -> Result<Option<Label>, RuntimeError> {
        self.sessions
            .get(&mascot_id)
            .map(|session| session.label)
            .ok_or(RuntimeError::MascotNotFound(mascot_id))
    }

    /// 更新 mascot 锚点。
    pub fn set_anchor(&mut self, mascot_id: i32, anchor: Anchor) -> Result<(), RuntimeError> {
        if !anchor.x.is_finite() || !anchor.y.is_finite() {
            return Err(RuntimeError::InvalidAnchor);
        }
        let session = self
            .sessions
            .get_mut(&mascot_id)
            .ok_or(RuntimeError::MascotNotFound(mascot_id))?;
        session.anchor = anchor;
        Ok(())
    }

    /// 更新 mascot 当前行为名称。
    pub fn set_active_behavior(
        &mut self,
        mascot_id: i32,
        behavior: Option<String>,
    ) -> Result<(), RuntimeError> {
        let session = self
            .sessions
            .get_mut(&mascot_id)
            .ok_or(RuntimeError::MascotNotFound(mascot_id))?;
        session.active_behavior = behavior;
        Ok(())
    }

    /// 使用单调时间点推进固定 40 ms 时钟。
    ///
    /// 第一次调用只建立时间基准，不执行 tick；之后倒退的时间点会被忽略，避免
    /// 系统时钟异常导致运行时倒退。
    pub fn tick_at(&mut self, now: Instant) -> TickReport {
        let elapsed = match self.last_tick {
            Some(previous) if now > previous => now.duration_since(previous),
            Some(_) => Duration::ZERO,
            None => Duration::ZERO,
        };
        if self.last_tick.is_none_or(|previous| now > previous) {
            self.last_tick = Some(now);
        }
        self.advance(elapsed)
    }

    /// [`tick_at`](Self::tick_at) 的简短别名。
    pub fn tick(&mut self, now: Instant) -> TickReport {
        self.tick_at(now)
    }

    /// 注入经过的时长并执行到期的固定 tick，适合测试和外部调度器。
    pub fn advance(&mut self, elapsed: Duration) -> TickReport {
        self.accumulator = self.accumulator.saturating_add(elapsed);
        let interval_nanos = TICK_INTERVAL.as_nanos();
        let due = self.accumulator.as_nanos() / interval_nanos;
        let remainder = self.accumulator.as_nanos() % interval_nanos;
        self.accumulator = Duration::from_nanos(remainder as u64);
        let ticks = u64::try_from(due).unwrap_or(u64::MAX);
        let subticks = ticks.saturating_mul(u64::from(SUBTICK_COUNT));
        self.tick_count = self.tick_count.saturating_add(ticks);
        self.subtick_count = self.subtick_count.saturating_add(subticks);
        TickReport {
            elapsed,
            ticks,
            subticks,
        }
    }

    /// 返回已经执行的主 tick 总数。
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// 返回已经执行的 subtick 总数。
    pub fn subtick_count(&self) -> u64 {
        self.subtick_count
    }

    /// 创建不携带可变引用的完整运行时快照。
    pub fn snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            sessions: self.list(),
            tick_count: self.tick_count,
            subtick_count: self.subtick_count,
        }
    }

    fn validate_spawn_request(&self, request: &SpawnRequest) -> Result<(), RuntimeError> {
        if request.name.trim().is_empty() {
            return Err(RuntimeError::EmptyName);
        }
        if request.data_id < 0 {
            return Err(RuntimeError::InvalidDataId(request.data_id));
        }
        if !request.anchor.x.is_finite() || !request.anchor.y.is_finite() {
            return Err(RuntimeError::InvalidAnchor);
        }
        Ok(())
    }

    fn next_available_label(&self) -> Result<Label, RuntimeError> {
        let mut candidate = self.next_label;
        loop {
            let label = Label::new(candidate);
            if !self.labels.contains_key(&label) {
                return Ok(label);
            }
            candidate = candidate
                .checked_add(1)
                .ok_or(RuntimeError::LabelExhausted)?;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use api::{Anchor, Label};

    use super::{Runtime, RuntimeError, SpawnRequest, TICK_INTERVAL};

    #[test]
    fn spawn_list_and_snapshot_keep_session_fields() {
        let mut runtime = Runtime::new();
        let request = SpawnRequest::new("Default", 42)
            .with_anchor(Anchor::new(12.5, 48.0).expect("finite anchor"))
            .with_label(Label::new(7))
            .with_active_behavior("Walk");

        let mascot = runtime.spawn(request).expect("spawn should succeed");

        assert_eq!(mascot.id(), 0);
        assert_eq!(mascot.name(), "Default");
        assert_eq!(mascot.data_id(), 42);
        assert_eq!(mascot.anchor(), Anchor::new(12.5, 48.0).unwrap());
        assert_eq!(mascot.label(), Some(Label::new(7)));
        assert_eq!(mascot.active_behavior(), Some("Walk"));
        assert_eq!(runtime.list(), vec![mascot.clone()]);

        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.sessions(), &[mascot]);
        assert_eq!(snapshot.tick_count(), 0);
    }

    #[test]
    fn spawn_rejects_invalid_input_and_duplicate_labels() {
        let mut runtime = Runtime::new();
        let empty_name = runtime.spawn(SpawnRequest::new("  ", 1));
        assert_eq!(empty_name, Err(RuntimeError::EmptyName));

        runtime
            .spawn(SpawnRequest::new("A", 1).with_label(Label::new(2)))
            .expect("first label should be accepted");
        let duplicate = runtime.spawn(SpawnRequest::new("B", 2).with_label(Label::new(2)));
        assert_eq!(duplicate, Err(RuntimeError::LabelInUse(Label::new(2))));
    }

    #[test]
    fn dismiss_and_dismiss_all_clear_sessions_and_labels() {
        let mut runtime = Runtime::new();
        let first = runtime
            .spawn(SpawnRequest::new("A", 1).with_label(Label::new(4)))
            .unwrap();
        let second = runtime.spawn(SpawnRequest::new("B", 2)).unwrap();

        assert_eq!(runtime.dismiss(first.id()).unwrap(), first);
        assert_eq!(
            runtime.dismiss(first.id()),
            Err(RuntimeError::MascotNotFound(first.id()))
        );
        assert_eq!(runtime.list(), vec![second]);

        let third = runtime.spawn(SpawnRequest::new("C", 3)).unwrap();
        assert_eq!(runtime.dismiss_all(), 2);
        assert!(runtime.list().is_empty());
        assert_eq!(
            runtime.dismiss(third.id()),
            Err(RuntimeError::MascotNotFound(third.id()))
        );
    }

    #[test]
    fn register_label_allocates_and_clears_transient_handles() {
        let mut runtime = Runtime::new();
        let first = runtime.spawn(SpawnRequest::new("A", 1)).unwrap();
        let second = runtime.spawn(SpawnRequest::new("B", 2)).unwrap();

        assert_eq!(runtime.register_label(first.id(), None).unwrap(), Label::new(0));
        assert_eq!(runtime.register_label(first.id(), None).unwrap(), Label::new(0));
        assert_eq!(
            runtime.register_label(second.id(), Some(Label::new(0))),
            Err(RuntimeError::LabelInUse(Label::new(0)))
        );
        runtime.clear_label(first.id()).unwrap();
        assert_eq!(runtime.label_for(first.id()).unwrap(), None);
        assert_eq!(runtime.register_label(second.id(), None).unwrap(), Label::new(1));
    }

    #[test]
    fn tick_uses_a_fixed_forty_millisecond_interval_and_four_subticks() {
        let mut runtime = Runtime::new();
        let start = Instant::now();

        assert_eq!(runtime.tick_at(start).ticks(), 0);
        assert_eq!(
            runtime.tick_at(start + Duration::from_millis(39)).ticks(),
            0
        );
        let first = runtime.tick_at(start + TICK_INTERVAL);
        assert_eq!(first.ticks(), 1);
        assert_eq!(first.subticks(), 4);
        assert_eq!(runtime.tick_count(), 1);
        assert_eq!(runtime.subtick_count(), 4);

        let second = runtime.tick_at(start + Duration::from_millis(121));
        assert_eq!(second.ticks(), 2);
        assert_eq!(second.subticks(), 8);
        assert_eq!(runtime.tick_count(), 3);
    }
}
