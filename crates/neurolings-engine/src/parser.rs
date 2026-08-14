//! 桌宠包 XML 解析：actions.xml / behaviors.xml → 动作树、行为列表与常量表。
//!
//! 解析基于 roxmltree；老式日文 Shimeji 标签与属性在读取时即时翻译，
//! 节点名、属性名与精确匹配的属性值都会被翻译。

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::action::animation::AnimationAction;
use crate::action::look::Look;
use crate::action::offset::Offset;
use crate::action::reference::Reference;
use crate::action::select::Select;
use crate::action::sequence::Sequence;
use crate::action::{Action, SharedAction, shared};
use crate::animation::Animation;
use crate::behavior::{Behavior, BehaviorList};
use crate::error::{EngineError, Result};
use crate::hotspot::{Hotspot, Shape};
use crate::math::Vec2;
use crate::pose::{Frame, Pose};
use crate::scripting::condition::Condition;
use crate::translator::translate_token;

fn tag_name(node: &roxmltree::Node) -> String {
    translate_token(node.tag_name().name()).to_string()
}

fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{feff}').unwrap_or(s)
}

fn all_attributes(
    node: &roxmltree::Node,
    defaults: &[(&str, &str)],
) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    for attr in node.attributes() {
        let name = translate_token(attr.name()).to_string();
        let value = translate_token(attr.value()).to_string();
        if map.contains_key(&name) {
            return Err(EngineError::Parse(format!("Duplicate attribute: {name}")));
        }
        map.insert(name, value);
    }
    for (key, value) in defaults {
        map.entry(key.to_string())
            .or_insert_with(|| value.to_string());
    }
    Ok(map)
}

/// 读取必需属性，缺失时报解析错误。
fn req_attr<'a>(map: &'a HashMap<String, String>, key: &str) -> Result<&'a String> {
    map.get(key)
        .ok_or_else(|| EngineError::Parse(format!("missing attribute: {key}")))
}

/// 宽松整数解析：取数字前缀，无法解析时返回 0。
fn parse_int_lenient(s: &str) -> i64 {
    let mut i = 0;
    let bytes = s.as_bytes();
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    s[..i].parse::<i64>().unwrap_or(0)
}

/// 严格前缀整数解析：允许前导空白与符号，取数字前缀；完全没有数字时报错。
fn parse_int_strict(s: &str) -> Result<i32> {
    let trimmed = s.trim_start();
    let bytes = trimmed.as_bytes();
    let mut i = 0;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }
    let digits_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_start {
        return Err(EngineError::Parse(format!("invalid integer: {s}")));
    }
    trimmed[..i]
        .parse::<i32>()
        .map_err(|_| EngineError::Parse(format!("invalid integer: {s}")))
}

fn parse_xml(xml: &str) -> Result<roxmltree::Document<'_>> {
    roxmltree::Document::parse(strip_bom(xml))
        .map_err(|e| EngineError::Parse(format!("XML error: {e}")))
}

pub struct Parser {
    action_refs: Vec<Rc<RefCell<Reference>>>,
    behavior_refs: Vec<Rc<Behavior>>,
    actions: HashMap<String, SharedAction>,
    pub behavior_list: BehaviorList,
    pub poses: Vec<Pose>,
    pub constants: HashMap<String, String>,
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

impl Parser {
    pub fn new() -> Self {
        Self {
            action_refs: Vec::new(),
            behavior_refs: Vec::new(),
            actions: HashMap::new(),
            behavior_list: BehaviorList::default(),
            poses: Vec::new(),
            constants: HashMap::new(),
        }
    }

    pub fn parse(&mut self, actions_xml: &str, behaviors_xml: &str) -> Result<()> {
        self.poses.clear();
        self.constants.clear();
        self.behavior_list = BehaviorList::default();
        self.actions.clear();
        self.action_refs.clear();
        self.behavior_refs.clear();

        self.parse_actions(actions_xml)?;
        self.parse_behaviors(behaviors_xml)?;

        self.action_refs.clear();
        self.behavior_refs.clear();
        self.actions.clear();
        Ok(())
    }

    fn parse_pose(&mut self, node: &roxmltree::Node) -> Result<Pose> {
        if tag_name(node) != "Pose" {
            return Err(EngineError::Parse("Expected Pose node".into()));
        }
        let attr = all_attributes(node, &[])?;
        if node.children().any(|c| c.is_element()) {
            return Err(EngineError::Parse("Non-empty Pose contents".into()));
        }
        let image = attr.get("Image").cloned().unwrap_or_default();
        let image_right = attr.get("ImageRight").cloned().unwrap_or_default();
        let sound = attr.get("Sound").cloned().unwrap_or_default();
        let anchor = attr
            .get("ImageAnchor")
            .map(|s| Vec2::from_str_lenient(s))
            .unwrap_or(Vec2::ZERO);
        let velocity = attr
            .get("Velocity")
            .map(|s| Vec2::from_str_lenient(s))
            .unwrap_or(Vec2::ZERO);
        let duration = parse_int_lenient(req_attr(&attr, "Duration")?);
        let pose = Pose {
            frame: Frame::new(image, image_right, sound, anchor),
            velocity,
            duration: duration as i32,
        };
        if !self.poses.iter().any(|p| p.frame.name == pose.frame.name) {
            self.poses.push(pose.clone());
        }
        Ok(pose)
    }

    fn parse_hotspot(node: &roxmltree::Node) -> Result<Option<Hotspot>> {
        if tag_name(node) != "Hotspot" {
            return Err(EngineError::Parse("Expected Hotspot node".into()));
        }
        let attr = all_attributes(node, &[])?;
        if node.children().any(|c| c.is_element()) {
            return Err(EngineError::Parse("Non-empty Hotspot contents".into()));
        }
        let shape = Shape::from_name(req_attr(&attr, "Shape")?);
        if shape == Shape::Invalid {
            return Ok(None);
        }
        let origin = Vec2::from_str_lenient(req_attr(&attr, "Origin")?);
        let size = Vec2::from_str_lenient(req_attr(&attr, "Size")?);
        let behavior = attr.get("Behavior").cloned().unwrap_or_default();
        if behavior.is_empty() {
            return Ok(None);
        }
        Ok(Some(Hotspot::new(shape, origin, size, behavior)))
    }

    fn parse_animation(&mut self, node: &roxmltree::Node) -> Result<Option<Rc<Animation>>> {
        if tag_name(node) != "Animation" {
            return Ok(None);
        }
        let cond_attr = node
            .attributes()
            .find(|a| translate_token(a.name()) == "Condition")
            .map(|a| translate_token(a.value()).to_string());
        let condition = match cond_attr {
            Some(js) => Condition::from(js.as_str()),
            None => Condition::Constant(true),
        };
        let mut poses = Vec::new();
        let mut hotspots = Vec::new();
        for sub in node.children().filter(|c| c.is_element()) {
            if tag_name(&sub) == "Hotspot" {
                if let Some(hotspot) = Self::parse_hotspot(&sub)? {
                    hotspots.push(hotspot);
                }
            } else {
                poses.push(self.parse_pose(&sub)?);
            }
        }
        if poses.is_empty() {
            return Err(EngineError::Parse("Animation has no Poses".into()));
        }
        let mut anim = Animation::new(poses, hotspots);
        anim.condition = condition;
        Ok(Some(Rc::new(anim)))
    }

    fn parse_action(&mut self, node: &roxmltree::Node, is_child: bool) -> Result<SharedAction> {
        let node_name = tag_name(node);
        let attributes = all_attributes(node, &[])?;

        if is_child && node_name == "ActionReference" {
            let reference = Rc::new(RefCell::new(Reference::default()));
            reference.borrow_mut().set_init_attr(attributes);
            self.action_refs.push(reference.clone());
            let coerced: SharedAction = reference;
            return Ok(coerced);
        }
        if node_name != "Action" {
            return Err(EngineError::Parse(format!(
                "Expected Action node, got {node_name}"
            )));
        }

        let mut action_type = req_attr(&attributes, "Type")?.clone();
        if action_type == "Embedded" {
            let class = req_attr(&attributes, "Class")?.clone();
            const PREFIX: &str = "com.group_finity.mascot.action.";
            if !class.starts_with(PREFIX) {
                return Err(EngineError::Parse("Invalid class name".into()));
            }
            action_type = class[PREFIX.len()..].to_string();
        }

        let result = match action_type.as_str() {
            "Select" | "Sequence" => self.parse_sequence_action(node, &action_type)?,
            "Offset" | "Look" => {
                if node.children().any(|c| c.is_element()) {
                    return Err(EngineError::Parse(
                        "Instant action with non-empty contents".into(),
                    ));
                }
                if action_type == "Offset" {
                    shared(Offset::default())
                } else {
                    shared(Look::default())
                }
            }
            other => self.parse_animation_action(node, other)?,
        };

        result.borrow_mut().set_init_attr(attributes.clone());
        if !is_child {
            let name = req_attr(&attributes, "Name")?.clone();
            self.actions.insert(name, result.clone());
        }
        Ok(result)
    }

    fn parse_sequence_action(
        &mut self,
        node: &roxmltree::Node,
        action_type: &str,
    ) -> Result<SharedAction> {
        let mut children = Vec::new();
        for sub in node.children().filter(|c| c.is_element()) {
            children.push(self.parse_action(&sub, true)?);
        }
        if children.is_empty() {
            return Err(EngineError::Parse("Sequence has no Actions".into()));
        }
        if action_type == "Select" {
            let mut select = Select::new();
            select.sequence_mut().actions = children;
            Ok(shared(select))
        } else {
            let mut seq = Sequence::default();
            seq.actions = children;
            Ok(shared(seq))
        }
    }

    fn parse_animation_action(
        &mut self,
        node: &roxmltree::Node,
        action_type: &str,
    ) -> Result<SharedAction> {
        use crate::action::animate::Animate;
        use crate::action::breed::Breed;
        use crate::action::dragged::Dragged;
        use crate::action::fall::Fall;
        use crate::action::interact::Interact;
        use crate::action::jump::Jump;
        use crate::action::movement::Move;
        use crate::action::movewithturn::MoveWithTurn;
        use crate::action::resist::Resist;
        use crate::action::scanmove::ScanMove;
        use crate::action::selfdestruct::SelfDestruct;
        use crate::action::stay::Stay;
        use crate::action::transform::Transform;
        use crate::action::turn::Turn;

        // 未单独实现的动作类型按经典别名归并到最近似的已实现类型。
        let canonical = match action_type {
            "Broadcast" | "ThrowIE" => "Animate",
            "BroadcastStay" => "Stay",
            "BroadcastMove" | "WalkWithIE" => "Move",
            "Regist" => "Resist", // not a typo
            "FallWithIE" => "Fall",
            other => other,
        };

        let mut animations = Vec::new();
        for anim_node in node.children().filter(|c| c.is_element()) {
            if let Some(anim) = self.parse_animation(&anim_node)? {
                animations.push(anim);
            }
        }
        macro_rules! collect {
            ($action:expr) => {{
                let mut action = $action;
                action.anim_mut().animations = animations;
                shared(action)
            }};
        }

        Ok(match canonical {
            "Jump" => collect!(Jump::default()),
            "Animate" => collect!(Animate::default()),
            "Breed" => collect!(Breed::default()),
            "Dragged" => collect!(Dragged::default()),
            "Resist" => collect!(Resist::default()),
            "Stay" => collect!(Stay::default()),
            "Move" => collect!(Move::default()),
            "Turn" => collect!(Turn::default()),
            "MoveWithTurn" => collect!(MoveWithTurn::default()),
            "Fall" => collect!(Fall::default()),
            "ScanMove" => collect!(ScanMove::default()),
            "Interact" => collect!(Interact::default()),
            "SelfDestruct" => collect!(SelfDestruct::default()),
            "Transform" => collect!(Transform::default()),
            other => return Err(EngineError::Parse(format!("Unrecognized type: {other}"))),
        })
    }

    fn parse_actions(&mut self, actions_xml: &str) -> Result<()> {
        let doc = parse_xml(actions_xml)?;
        let mascot = doc.root_element();
        if tag_name(&mascot) != "Mascot" {
            return Err(EngineError::Parse("Root node is not named Mascot".into()));
        }
        for action_list in mascot.children().filter(|c| c.is_element()) {
            if tag_name(&action_list) != "ActionList" {
                continue;
            }
            for action in action_list.children().filter(|c| c.is_element()) {
                self.parse_action(&action, false)?;
            }
        }
        // 没有 ActionList 不视为解析错误：动作表为空，行为连接阶段会给出具体错误。

        // 链接动作引用（允许引用在后面定义的动作）。
        let mut all_linked = true;
        for reference in self.action_refs.clone() {
            let name = reference
                .borrow()
                .base()
                .init_attr
                .get("Name")
                .cloned()
                .unwrap_or_default();
            match self.actions.get(&name) {
                Some(target) => reference.borrow_mut().target = Some(target.clone()),
                None => {
                    eprintln!("Referenced unknown action: {name}");
                    all_linked = false;
                }
            }
        }
        if !all_linked {
            return Err(EngineError::Parse("Failed to link ActionReferences".into()));
        }
        Ok(())
    }

    fn parse_behavior_list(
        &mut self,
        root: &roxmltree::Node,
        allow_references: bool,
    ) -> Result<BehaviorList> {
        let mut list = BehaviorList::default();
        for node in root.children().filter(|c| c.is_element()) {
            let name = tag_name(&node);
            if name == "Behavior" || name == "BehaviorReference" {
                let reference = name == "BehaviorReference";
                if reference && !allow_references {
                    return Err(EngineError::Parse("allow_references == false".into()));
                }
                let attr = all_attributes(
                    &node,
                    &[
                        ("Name", ""),
                        ("Condition", "true"),
                        ("Hidden", "false"),
                        ("Frequency", "0"),
                    ],
                )?;
                let frequency = parse_int_strict(&attr["Frequency"])?;
                let hidden = attr["Hidden"] == "true";
                let condition = Condition::from(attr["Condition"].as_str());
                let mut behavior =
                    Behavior::new(attr["Name"].clone(), frequency, hidden, condition);

                let next_lists: Vec<roxmltree::Node> = node
                    .children()
                    .filter(|c| c.is_element() && tag_name(c) == "NextBehaviorList")
                    .collect();
                if !next_lists.is_empty() {
                    if next_lists.len() > 1 {
                        return Err(EngineError::Parse("Multiple NextBehaviorList nodes".into()));
                    }
                    let subnode = &next_lists[0];
                    behavior.add_next = subnode
                        .attributes()
                        .find(|a| translate_token(a.name()) == "Add")
                        .is_none_or(|a| translate_token(a.value()) == "true");
                    behavior.next_list = Some(self.parse_behavior_list(subnode, true)?);
                }

                let behavior = Rc::new(behavior);
                list.children.push(behavior.clone());
                if reference {
                    self.behavior_refs.push(behavior);
                }
            } else if name == "Condition" {
                let cond_attr = node
                    .attributes()
                    .find(|a| translate_token(a.name()) == "Condition")
                    .map(|a| translate_token(a.value()).to_string());
                let condition = match cond_attr {
                    Some(js) => Condition::from(js.as_str()),
                    None => Condition::Constant(true),
                };
                let mut sublist = self.parse_behavior_list(&node, allow_references)?;
                sublist.condition = condition;
                list.sublists.push(sublist);
            }
            // 未知节点直接忽略。
        }
        Ok(list)
    }

    fn connect_actions(&self, behaviors: &BehaviorList) -> Result<()> {
        for child in &behaviors.children {
            if child.referenced.borrow().is_some() {
                continue;
            }
            let action = self.actions.get(&child.name).cloned().ok_or_else(|| {
                EngineError::Parse(format!("no action for behavior: {}", child.name))
            })?;
            *child.action.borrow_mut() = Some(action);
            if let Some(next_list) = &child.next_list {
                // NextBehaviorList 里嵌套行为本不合规，但确有桌宠包这么写，照常连接。
                self.connect_actions(next_list)?;
            }
        }
        for sublist in &behaviors.sublists {
            self.connect_actions(sublist)?;
        }
        Ok(())
    }

    fn parse_behaviors(&mut self, behaviors_xml: &str) -> Result<()> {
        let doc = parse_xml(behaviors_xml)?;
        let mascot = doc.root_element();
        if tag_name(&mascot) != "Mascot" {
            return Err(EngineError::Parse("Root node is not named Mascot".into()));
        }
        for node in mascot.children().filter(|c| c.is_element()) {
            match tag_name(&node).as_str() {
                "Constant" => {
                    let attr = all_attributes(&node, &[])?;
                    let name = attr
                        .get("Name")
                        .cloned()
                        .ok_or_else(|| EngineError::Parse("Invalid constant".into()))?;
                    let value = attr
                        .get("Value")
                        .cloned()
                        .ok_or_else(|| EngineError::Parse("Invalid constant".into()))?;
                    if self.constants.contains_key(&name) {
                        return Err(EngineError::Parse(format!(
                            "Multiple constants with same name: {name}"
                        )));
                    }
                    self.constants.insert(name, value);
                }
                "BehaviorList" => {
                    let sublist = self.parse_behavior_list(&node, false)?;
                    self.behavior_list.sublists.push(sublist);
                }
                other => {
                    return Err(EngineError::Parse(format!(
                        "Invalid tag in behaviours XML: {other}"
                    )));
                }
            }
        }

        // 链接行为引用；目标不存在时回退到 Fall。
        for reference in self.behavior_refs.clone() {
            let target = match self.behavior_list.find(&reference.name) {
                Some(t) => t,
                None => self.behavior_list.find("Fall").ok_or_else(|| {
                    EngineError::Parse(format!("invalid behavior reference: {}", reference.name))
                })?,
            };
            *reference.referenced.borrow_mut() = Some(target);
        }

        self.connect_actions(&self.behavior_list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mascot_pack_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mascot_pack")
    }

    #[test]
    fn parses_default_mascot_pack() {
        let base = mascot_pack_path().join("Default");
        let actions = std::fs::read_to_string(base.join("actions.xml")).unwrap();
        let behaviors = std::fs::read_to_string(base.join("behaviors.xml")).unwrap();
        let mut parser = Parser::new();
        parser.parse(&actions, &behaviors).unwrap();
        assert!(parser.poses.len() > 10, "poses parsed");
        assert!(
            parser.behavior_list.find("Fall").is_some(),
            "Fall behavior exists"
        );
    }

    #[test]
    fn parses_constants_and_references() {
        let actions_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
            <Mascot xmlns="http://www.group-finity.com/Mascot">
              <ActionList>
                <Action Name="Stand" Type="Stay" BorderType="Floor">
                  <Animation><Pose Image="/shime1.png" ImageAnchor="64,128" Velocity="0,0" Duration="250"/></Animation>
                </Action>
                <Action Name="Fall" Type="Fall">
                  <Animation><Pose Image="/shime1.png" ImageAnchor="64,128" Velocity="0,0" Duration="250"/></Animation>
                </Action>
              </ActionList>
            </Mascot>"#;
        let behaviors_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
            <Mascot xmlns="http://www.group-finity.com/Mascot">
              <Constant Name="Speed" Value="2"/>
              <BehaviorList>
                <Behavior Name="Stand" Frequency="1">
                  <NextBehaviorList>
                    <BehaviorReference Name="Stand"/>
                  </NextBehaviorList>
                </Behavior>
                <Behavior Name="Fall" Frequency="0"/>
              </BehaviorList>
            </Mascot>"#;
        let mut parser = Parser::new();
        parser.parse(actions_xml, behaviors_xml).unwrap();
        assert_eq!(parser.constants.get("Speed").map(String::as_str), Some("2"));
        let flat = parser.behavior_list.flatten_unconditional();
        assert_eq!(flat.len(), 2);
        let stand = &flat[0];
        let next_list = stand.next_list.as_ref().expect("NextBehaviorList parsed");
        let reference = &next_list.children[0];
        assert!(reference.referenced.borrow().is_some(), "reference linked");
        // 引用未知名称时回退到 Fall。
    }
}
