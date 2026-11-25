use std::rc::Rc;

use gpui::{
    App, Bounds, DispatchPhase, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PaintQuad, Pixels, Rgba, Window, canvas, div, point, prelude::*, px, rgb, size,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliderId {
    HwBrightness,
    LiveBrightness,
    SaveBrightness,
    LayerSpeed,
    LayerOpacity,
    LayerAngle,
    ColorR,
    ColorG,
    ColorB,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliderPhase {
    Start,
    Move,
    Commit,
}

pub fn render_slider(
    fraction: f32,
    fill_color: Rgba,
    is_dragging: bool,
    enabled: bool,
    on_change: impl Fn(f32, SliderPhase, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let frac = fraction.clamp(0.0, 1.0);
    let cb = Rc::new(on_change);

    div()
        .w_full()
        .h(px(22.0))
        .when(enabled, |d| d.cursor_pointer())
        .child(
            canvas(
                |_, _, _| {},
                move |bounds: Bounds<Pixels>, _, window: &mut Window, _cx: &mut App| {
                    let thumb_radius = px(7.0);
                    let track_height = px(6.0);
                    let usable_w = (bounds.size.width - thumb_radius * 2.0).max(px(1.0));
                    let center_y = bounds.origin.y + bounds.size.height * 0.5;
                    let track_left = bounds.origin.x + thumb_radius;
                    let track_top = center_y - track_height * 0.5;

                    let track_bg = if enabled { rgb(0x2a2f3d) } else { rgb(0x1f232d) };
                    let active_fill = if enabled { fill_color } else { rgb(0x4b5563) };
                    let thumb_bg = if !enabled {
                        rgb(0x6b7280)
                    } else if is_dragging {
                        rgb(0xffffff)
                    } else {
                        rgb(0xf3f4f6)
                    };

                    // Background track
                    window.paint_quad(
                        PaintQuad::new(Bounds::new(
                            point(track_left, track_top),
                            size(usable_w, track_height),
                        ))
                        .background(track_bg)
                        .corner_radii(px(3.0)),
                    );

                    // Filled portion
                    let fill_w = (usable_w * frac).max(px(0.0));
                    if fill_w > px(0.5) {
                        window.paint_quad(
                            PaintQuad::new(Bounds::new(
                                point(track_left, track_top),
                                size(fill_w, track_height),
                            ))
                            .background(active_fill)
                            .corner_radii(px(3.0)),
                        );
                    }

                    // Thumb
                    let thumb_x = track_left + usable_w * frac;
                    let thumb_size = if is_dragging { px(16.0) } else { px(14.0) };
                    window.paint_quad(
                        PaintQuad::new(Bounds::centered_at(
                            point(thumb_x, center_y),
                            size(thumb_size, thumb_size),
                        ))
                        .background(thumb_bg)
                        .corner_radii(thumb_size * 0.5)
                        .border_widths(px(2.0))
                        .border_color(if is_dragging { active_fill } else { rgb(0x111318) }),
                    );

                    if !enabled {
                        return;
                    }

                    let calc_frac = move |pos_x: Pixels| -> f32 {
                        ((pos_x - track_left) / usable_w).clamp(0.0, 1.0)
                    };

                    let cb_down = Rc::clone(&cb);
                    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                        if phase == DispatchPhase::Bubble
                            && event.button == MouseButton::Left
                            && bounds.contains(&event.position)
                        {
                            let v = calc_frac(event.position.x);
                            cb_down(v, SliderPhase::Start, window, cx);
                        }
                    });

                    if is_dragging {
                        let cb_move = Rc::clone(&cb);
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Bubble {
                                let v = calc_frac(event.position.x);
                                if event.pressed_button == Some(MouseButton::Left) {
                                    cb_move(v, SliderPhase::Move, window, cx);
                                } else {
                                    cb_move(v, SliderPhase::Commit, window, cx);
                                }
                            }
                        });

                        let cb_up = Rc::clone(&cb);
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                                let v = calc_frac(event.position.x);
                                cb_up(v, SliderPhase::Commit, window, cx);
                            }
                        });
                    }
                },
            )
            .size_full(),
        )
}
