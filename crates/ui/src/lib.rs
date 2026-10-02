//! 管理器窗口、透明 mascot viewport 和托盘交互入口。

use std::time::Instant;

use eframe::egui;
use services::CommandService;

/// GUI 内存状态；窗口关闭和 tick 均在 UI 线程执行。
#[derive(Debug)]
pub struct ManagerUi {
    service: CommandService,
    selected: Option<i32>,
    show_manager: bool,
    last_tick: Instant,
}

impl Default for ManagerUi {
    fn default() -> Self {
        Self {
            service: CommandService::new(),
            selected: None,
            show_manager: true,
            last_tick: Instant::now(),
        }
    }
}

impl ManagerUi {
    /// 创建主窗口状态。
    pub fn new() -> Self {
        Self::default()
    }

    /// 返回当前是否显示管理器窗口。
    pub fn show_manager(&self) -> bool {
        self.show_manager
    }
}

impl eframe::App for ManagerUi {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        if now.duration_since(self.last_tick) >= runtime::TICK_INTERVAL {
            self.service.runtime_mut().tick_at(now);
            self.last_tick = now;
        }
        if !self.show_manager {
            return;
        }
        egui::TopBottomPanel::top("toolbar").show(context, |ui| {
            ui.horizontal(|ui| {
                ui.heading("NeurolingsCE");
                if ui.button("召唤").clicked() {
                    let _ = self.service.execute(
                        api::ApiRequest::new(api::Command::SpawnMascot)
                            .with_field("name", serde_json::json!("Default Mascot")),
                    );
                }
                if ui.button("全部关闭").clicked() {
                    let _ = self
                        .service
                        .execute(api::ApiRequest::new(api::Command::DismissAllMascots));
                }
            });
        });
        egui::CentralPanel::default().show(context, |ui| {
            ui.heading("桌宠管理器");
            for mascot in self.service.runtime().list() {
                let selected = self.selected == Some(mascot.id());
                if ui
                    .selectable_label(selected, format!("{}  #{}", mascot.name(), mascot.id()))
                    .clicked()
                {
                    self.selected = Some(mascot.id());
                }
                ui.label(format!(
                    "位置 ({:.0}, {:.0})",
                    mascot.anchor().x,
                    mascot.anchor().y
                ));
            }
            ui.separator();
            ui.label("Mascot viewport");
            let available = ui.available_size();
            let (rect, _) = ui.allocate_exact_size(available, egui::Sense::hover());
            let painter = ui.painter_at(rect);
            for mascot in self.service.runtime().list() {
                let center = rect.left_top()
                    + egui::vec2(
                        mascot.anchor().x as f32 % rect.width().max(1.0),
                        mascot.anchor().y as f32 % rect.height().max(1.0),
                    );
                painter.circle_filled(center, 18.0, egui::Color32::from_rgb(102, 170, 255));
                painter.text(
                    center + egui::vec2(-12.0, 22.0),
                    egui::Align2::LEFT_TOP,
                    mascot.name(),
                    egui::FontId::proportional(12.0),
                    egui::Color32::WHITE,
                );
            }
        });
        context.request_repaint_after(runtime::TICK_INTERVAL);
    }
}

/// 启动桌面 GUI。
pub fn run() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "NeurolingsCE",
        options,
        Box::new(|_creation_context| Ok(Box::new(ManagerUi::new()))),
    )
}

#[cfg(test)]
mod tests {
    use super::ManagerUi;

    #[test]
    fn manager_ui_starts_with_manager_visible() {
        assert!(ManagerUi::new().show_manager());
    }
}
