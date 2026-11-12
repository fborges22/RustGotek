use super::visual::{self, Appearance, Icon};
use eframe::egui::{
    self, Align2, Color32, CursorIcon, FontId, Id, Rect, ResizeDirection, Sense, Stroke,
    StrokeKind, Vec2, ViewportCommand,
};

pub fn title_bar(ctx: &egui::Context, appearance: &mut Appearance, busy: bool, workspace: &str) {
    let colors = visual::colors(ctx);
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    egui::TopBottomPanel::top("custom-title-bar")
        .exact_height(48.)
        .frame(egui::Frame::new().fill(colors.surface))
        .show(ctx, |ui| {
            let rect = ui.max_rect();
            ui.allocate_rect(rect, Sense::hover());
            let controls_left = rect.right() - 3. * 44. - 4.;
            let theme_rect = Rect::from_min_size(
                egui::pos2(controls_left - 128., rect.top() + 8.),
                egui::vec2(116., 32.),
            );
            let drag_rect =
                Rect::from_min_max(rect.min, egui::pos2(theme_rect.left() - 10., rect.bottom()));
            let drag = ui.interact(drag_rect, Id::new("title-drag"), Sense::click_and_drag());
            let mark = Rect::from_center_size(
                egui::pos2(rect.left() + 28., rect.center().y),
                Vec2::splat(28.),
            );
            ui.painter().rect_filled(mark, 7, visual::BLUE);
            visual::icon(ui.painter(), mark.shrink(5.), Icon::Disk, Color32::WHITE);
            ui.painter().text(
                egui::pos2(rect.left() + 52., rect.center().y),
                Align2::LEFT_CENTER,
                "RustGotek",
                FontId::proportional(18.),
                colors.ink,
            );
            ui.painter().text(
                egui::pos2(rect.left() + 155., rect.center().y),
                Align2::LEFT_CENTER,
                format!("/  {workspace}"),
                FontId::proportional(12.),
                colors.muted,
            );
            if drag.double_clicked_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
            } else if drag.drag_started_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(ViewportCommand::StartDrag);
            }
            drag.context_menu(|ui| {
                if ui
                    .button(if maximized {
                        "Restore window"
                    } else {
                        "Maximize window"
                    })
                    .clicked()
                {
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                    ui.close();
                }
                if ui.button("Minimize window").clicked() {
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(!busy, egui::Button::new("Close window"))
                    .clicked()
                {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                    ui.close();
                }
            });
            ui.scope_builder(egui::UiBuilder::new().max_rect(theme_rect), |ui| {
                let glyph = match appearance {
                    Appearance::System => Icon::Monitor,
                    Appearance::Light => Icon::Sun,
                    Appearance::Dark => Icon::Moon,
                };
                let response = visual::small_button(ui, glyph, appearance.name())
                    .on_hover_text("Appearance: System, Light or Dark");
                egui::Popup::menu(&response).show(|ui| {
                    ui.label(
                        egui::RichText::new("APPEARANCE")
                            .small()
                            .color(colors.muted),
                    );
                    for option in [Appearance::System, Appearance::Light, Appearance::Dark] {
                        if ui
                            .selectable_label(*appearance == option, option.name())
                            .clicked()
                        {
                            *appearance = option;
                            ctx.set_theme(option.preference());
                            ctx.request_repaint();
                            ui.close();
                        }
                    }
                    ui.separator();
                    ui.label(
                        egui::RichText::new("System follows your desktop theme.")
                            .small()
                            .color(colors.muted),
                    );
                });
            });
            for (n, glyph, label, enabled, command) in [
                (
                    0,
                    Icon::Minimize,
                    "Minimize",
                    true,
                    ViewportCommand::Minimized(true),
                ),
                (
                    1,
                    if maximized {
                        Icon::RestoreWindow
                    } else {
                        Icon::Maximize
                    },
                    if maximized { "Restore" } else { "Maximize" },
                    true,
                    ViewportCommand::Maximized(!maximized),
                ),
                (2, Icon::Close, "Close", !busy, ViewportCommand::Close),
            ] {
                let button_rect = Rect::from_min_size(
                    egui::pos2(controls_left + n as f32 * 44., rect.top() + 4.),
                    egui::vec2(44., 40.),
                );
                let response = {
                    let mut control_ui = ui.new_child(egui::UiBuilder::new().max_rect(button_rect));
                    if !enabled {
                        control_ui.disable();
                    }
                    let ui = &mut control_ui;
                    let response =
                        ui.interact(button_rect, Id::new(("window-control", n)), Sense::click());
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
                    });
                    let active = response.hovered() || response.has_focus();
                    let dangerous = n == 2 && active;
                    if active {
                        ui.painter().rect_filled(
                            button_rect,
                            4,
                            if dangerous {
                                Color32::from_rgb(193, 49, 67)
                            } else {
                                colors.pale
                            },
                        );
                    }
                    if response.has_focus() {
                        ui.painter().rect_stroke(
                            button_rect,
                            4,
                            Stroke::new(1.0f32, colors.accent),
                            StrokeKind::Inside,
                        );
                    }
                    visual::icon(
                        ui.painter(),
                        Rect::from_center_size(button_rect.center(), Vec2::splat(18.)),
                        glyph,
                        if dangerous {
                            Color32::WHITE
                        } else if enabled {
                            colors.ink
                        } else {
                            colors.disabled
                        },
                    );
                    response
                };
                if response
                    .on_hover_text(label)
                    .on_disabled_hover_text(
                        "Wait for the current operation to finish before closing.",
                    )
                    .clicked()
                {
                    ctx.send_viewport_cmd(command);
                }
            }
            ui.painter().line_segment(
                [rect.left_bottom(), rect.right_bottom()],
                Stroke::new(1.0f32, colors.border),
            );
        });
}

/// Resize handles around an undecorated window. The OS performs the resize.
pub fn resize_borders(ctx: &egui::Context) {
    if ctx.input(|i| {
        i.viewport().maximized.unwrap_or(false) || i.viewport().fullscreen.unwrap_or(false)
    }) {
        return;
    }
    let rect = ctx.viewport_rect();
    let edge = 4.;
    let corner = 10.;
    let handles = [
        (
            Rect::from_min_size(rect.min, Vec2::splat(corner)),
            ResizeDirection::NorthWest,
            CursorIcon::ResizeNwSe,
        ),
        (
            Rect::from_min_size(
                rect.right_top() - egui::vec2(corner, 0.),
                Vec2::splat(corner),
            ),
            ResizeDirection::NorthEast,
            CursorIcon::ResizeNeSw,
        ),
        (
            Rect::from_min_size(
                rect.left_bottom() - egui::vec2(0., corner),
                Vec2::splat(corner),
            ),
            ResizeDirection::SouthWest,
            CursorIcon::ResizeNeSw,
        ),
        (
            Rect::from_min_size(rect.max - Vec2::splat(corner), Vec2::splat(corner)),
            ResizeDirection::SouthEast,
            CursorIcon::ResizeNwSe,
        ),
        (
            Rect::from_min_max(
                rect.min + egui::vec2(corner, 0.),
                rect.right_top() + egui::vec2(-corner, edge),
            ),
            ResizeDirection::North,
            CursorIcon::ResizeVertical,
        ),
        (
            Rect::from_min_max(
                rect.left_bottom() + egui::vec2(corner, -edge),
                rect.max - egui::vec2(corner, 0.),
            ),
            ResizeDirection::South,
            CursorIcon::ResizeVertical,
        ),
        (
            Rect::from_min_max(
                rect.min + egui::vec2(0., corner),
                rect.left_bottom() + egui::vec2(edge, -corner),
            ),
            ResizeDirection::West,
            CursorIcon::ResizeHorizontal,
        ),
        (
            Rect::from_min_max(
                rect.right_top() + egui::vec2(-edge, corner),
                rect.max - egui::vec2(0., corner),
            ),
            ResizeDirection::East,
            CursorIcon::ResizeHorizontal,
        ),
    ];
    for (index, (rect, direction, cursor)) in handles.into_iter().enumerate() {
        egui::Area::new(Id::new(("window-resize", index)))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .movable(false)
            .constrain(false)
            .show(ctx, |ui| {
                ui.set_min_size(rect.size());
                let (_, response) = ui.allocate_exact_size(rect.size(), Sense::drag());
                let response = response.on_hover_cursor(cursor);
                if response.is_pointer_button_down_on()
                    && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Primary))
                {
                    ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(
        ctx: &egui::Context,
        appearance: &mut Appearance,
        busy: bool,
        maximized: bool,
        events: Vec<egui::Event>,
    ) -> Vec<ViewportCommand> {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280., 800.),
            )),
            events,
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .maximized = Some(maximized);
        let output = ctx.run(input, |ctx| {
            title_bar(ctx, appearance, busy, "Image library");
            assert_eq!(
                ctx.available_rect().top(),
                48.,
                "title controls must not expand the title bar"
            );
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("Window contents");
            });
            resize_borders(ctx);
        });
        output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .clone()
    }
    fn press(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }
    #[test]
    fn title_buttons_emit_native_commands_and_busy_close_is_disabled() {
        for (pos, busy, maximized, expected) in [
            (
                egui::pos2(1166., 24.),
                false,
                false,
                Some(ViewportCommand::Minimized(true)),
            ),
            (
                egui::pos2(1210., 24.),
                false,
                false,
                Some(ViewportCommand::Maximized(true)),
            ),
            (
                egui::pos2(1210., 24.),
                false,
                true,
                Some(ViewportCommand::Maximized(false)),
            ),
            (
                egui::pos2(1254., 24.),
                false,
                false,
                Some(ViewportCommand::Close),
            ),
            (egui::pos2(1254., 24.), true, false, None),
        ] {
            let ctx = egui::Context::default();
            visual::apply(&ctx);
            let mut appearance = Appearance::System;
            frame(&ctx, &mut appearance, busy, maximized, vec![]);
            frame(&ctx, &mut appearance, busy, maximized, vec![]);
            let mut commands = frame(
                &ctx,
                &mut appearance,
                busy,
                maximized,
                vec![egui::Event::PointerMoved(pos), press(pos, true)],
            );
            commands.extend(frame(
                &ctx,
                &mut appearance,
                busy,
                maximized,
                vec![press(pos, false)],
            ));
            if let Some(expected) = expected {
                assert!(commands.contains(&expected), "{commands:?}");
            } else {
                assert!(!commands.contains(&ViewportCommand::Close));
            }
        }
    }
    #[test]
    fn custom_title_drag_and_edge_resize_dispatch_to_window_manager() {
        let ctx = egui::Context::default();
        visual::apply(&ctx);
        let mut appearance = Appearance::Light;
        for _ in 0..2 {
            frame(&ctx, &mut appearance, false, false, vec![]);
        }
        let pos = egui::pos2(400., 24.);
        frame(
            &ctx,
            &mut appearance,
            false,
            false,
            vec![egui::Event::PointerMoved(pos), press(pos, true)],
        );
        let commands = frame(
            &ctx,
            &mut appearance,
            false,
            false,
            vec![egui::Event::PointerMoved(pos + egui::vec2(25., 0.))],
        );
        assert!(
            commands.contains(&ViewportCommand::StartDrag),
            "{commands:?}"
        );
        frame(&ctx, &mut appearance, false, false, vec![press(pos, false)]);
        let pos = egui::pos2(1., 400.);
        let commands = frame(
            &ctx,
            &mut appearance,
            false,
            false,
            vec![egui::Event::PointerMoved(pos), press(pos, true)],
        );
        assert!(
            commands.contains(&ViewportCommand::BeginResize(ResizeDirection::West)),
            "{commands:?}"
        );
    }
}
