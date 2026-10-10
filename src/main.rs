#![windows_subsystem = "windows"]

use eframe::egui;

fn main() {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "NeurolingsCE",
        native_options,
        Box::new(|cc| Ok(Box::new(NeurolingsCE::new(cc))))
    ).expect("The program cannot run properly. Please check if you do not have permission to run or if it is damaged");
}

#[derive(Default)]
struct NeurolingsCE {}

impl NeurolingsCE {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::default()
    }
}

impl eframe::App for NeurolingsCE {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Hello World!");
        });
    }
}