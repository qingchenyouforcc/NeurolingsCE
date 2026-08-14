//! --smoke 模式与 CI 使用的无头窗口桩。

use neurolings_platform::{MascotWindow, PlatformResult, Point};

pub struct HeadlessWindow;

impl MascotWindow for HeadlessWindow {
    fn update_frame(
        &mut self,
        _bitmap_bgra_premul: &[u8],
        _width: u32,
        _height: u32,
        _top_left: Point,
    ) -> PlatformResult<()> {
        Ok(())
    }
}
