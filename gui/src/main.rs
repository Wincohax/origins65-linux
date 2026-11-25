mod app;
mod driver;
mod layout;
mod model;
mod slider;

use gpui::{
    App, Bounds, QuitMode, TitlebarOptions, WindowBounds, WindowOptions, prelude::*, px, size,
};

use crate::app::Origins65App;

fn main() {
    gpui_platform::application()
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(move |cx: &mut App| {
            let bounds = Bounds::centered(None, size(px(1260.0), px(820.0)), cx);
            let titlebar = TitlebarOptions {
                title: Some("HyperX Alloy Origins 65 — Lighting Studio".into()),
                ..Default::default()
            };
            cx.open_window(
                WindowOptions::default()
                    .window_bounds(WindowBounds::Windowed(bounds))
                    .window_min_size(size(px(940.0), px(620.0)))
                    .titlebar(Some(titlebar)),
                |_window, cx| cx.new(Origins65App::new),
            )
            .expect("failed to open window");
            cx.activate(true);
        });
}
