//! 窗口与事件循环宿主。
//!
//! 负责创建窗口、驱动事件循环，并把界面内容挂到窗口上。

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

/// 应用状态：持有主窗口并处理其事件。
struct App {
    window: Option<Window>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("NeurolingsCE")
                    .with_inner_size(winit::dpi::LogicalSize::new(1100.0, 720.0)),
            )
            .expect("创建主窗口失败");
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            _ => {}
        }
    }
}

/// 启动事件循环，直到主窗口关闭。
pub fn run() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = App { window: None };
    event_loop.run_app(&mut app)?;
    Ok(())
}
