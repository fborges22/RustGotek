use eframe::egui::{self, Color32, FontId, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

pub const INK: Color32 = Color32::from_rgb(31, 42, 62);
pub const MUTED: Color32 = Color32::from_rgb(101, 115, 138);
pub const BLUE: Color32 = Color32::from_rgb(49, 94, 231);
pub const PALE: Color32 = Color32::from_rgb(233, 239, 255);
pub const BORDER: Color32 = Color32::from_rgb(221, 227, 237);
pub const PAPER: Color32 = Color32::from_rgb(246, 248, 252);
pub const NAVY: Color32 = Color32::from_rgb(24, 35, 55);
pub const GREEN: Color32 = Color32::from_rgb(28, 124, 98);
pub const AMBER: Color32 = Color32::from_rgb(151, 94, 15);
pub const RED: Color32 = Color32::from_rgb(187, 58, 73);

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.override_text_color = Some(INK);
    style.visuals.panel_fill = PAPER;
    style.visuals.window_fill = Color32::WHITE;
    style.visuals.window_stroke = Stroke::new(1.0f32, BORDER);
    style.visuals.window_corner_radius = 12.into();
    style.visuals.extreme_bg_color = Color32::WHITE;
    style.visuals.faint_bg_color = Color32::from_rgb(249, 250, 253);
    style.visuals.selection.bg_fill = PALE;
    style.visuals.selection.stroke = Stroke::new(1.0f32, BLUE);
    style.visuals.hyperlink_color = BLUE;
    style.visuals.warn_fg_color = AMBER;
    style.visuals.error_fg_color = RED;
    style.visuals.widgets.inactive.bg_fill = Color32::WHITE;
    style.visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0f32, BORDER);
    style.visuals.widgets.hovered.bg_fill = PALE;
    style.visuals.widgets.hovered.weak_bg_fill = PALE;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0f32, BLUE);
    style.visuals.widgets.active.bg_fill = PALE;
    style.visuals.widgets.active.weak_bg_fill = PALE;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0f32, BLUE);
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.noninteractive,
    ] {
        widget.fg_stroke = Stroke::new(1.3f32, INK);
        widget.corner_radius = 6.into();
    }
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, BORDER);
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.interact_size.y = 32.0;
    style.spacing.window_margin = 20.into();
    for text in [egui::TextStyle::Body, egui::TextStyle::Button] {
        style.text_styles.insert(text, FontId::proportional(14.0));
    }
    style
        .text_styles
        .insert(egui::TextStyle::Small, FontId::proportional(12.0));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, FontId::proportional(25.0));
    ctx.set_style(style);
}

#[derive(Clone, Copy)]
pub enum Icon {
    Edit,
    Disk,
    Add,
    Folder,
    Library,
    Drive,
    Import,
    Export,
    Replace,
    Trash,
    Backup,
    Convert,
    Tools,
    Search,
    Refresh,
    Check,
    Log,
    Close,
    Up,
}

/// A small consistent outline icon set, drawn in a 24-unit coordinate space.
pub fn icon(p: &egui::Painter, rect: Rect, glyph: Icon, color: Color32) {
    let origin = rect.min;
    let scale = rect.width() / 24.0;
    let point = |x: f32, y: f32| origin + egui::vec2(x, y) * scale;
    let stroke = Stroke::new(1.7 * scale, color);
    let line = |pts: &[(f32, f32)]| {
        p.line(pts.iter().map(|&(x, y)| point(x, y)).collect(), stroke);
    };
    let box_at = |x, y, w, h| {
        p.rect_stroke(
            Rect::from_min_size(point(x, y), egui::vec2(w, h) * scale),
            2.0 * scale,
            stroke,
            StrokeKind::Middle,
        );
    };
    match glyph {
        Icon::Edit => {
            line(&[
                (4., 20.),
                (5., 14.),
                (17., 2.),
                (22., 7.),
                (10., 19.),
                (4., 20.),
            ]);
            line(&[(14., 5.), (19., 10.)]);
        }
        Icon::Disk | Icon::Add => {
            line(&[
                (4., 3.),
                (17., 3.),
                (21., 7.),
                (21., 21.),
                (3., 21.),
                (3., 3.),
                (4., 3.),
            ]);
            box_at(7., 3., 9., 6.);
            box_at(7., 14., 10., 7.);
            if matches!(glyph, Icon::Add) {
                p.circle_filled(point(19., 18.), 6.0 * scale, BLUE);
                let white = Stroke::new(1.7 * scale, Color32::WHITE);
                p.line_segment([point(16., 18.), point(22., 18.)], white);
                p.line_segment([point(19., 15.), point(19., 21.)], white);
            }
        }
        Icon::Folder => {
            line(&[
                (3., 7.),
                (3., 4.),
                (9., 4.),
                (12., 7.),
                (21., 7.),
                (21., 20.),
                (3., 20.),
                (3., 7.),
                (21., 7.),
            ]);
        }
        Icon::Library => {
            box_at(3., 4., 5., 16.);
            box_at(10., 4., 5., 16.);
            line(&[(17., 5.), (20., 4.), (23., 19.), (20., 20.), (17., 5.)]);
        }
        Icon::Drive => {
            box_at(3., 5., 18., 14.);
            line(&[(3., 14.), (21., 14.)]);
            p.circle_filled(point(17., 16.5), scale, color);
        }
        Icon::Import | Icon::Export => {
            line(&[(4., 15.), (4., 21.), (20., 21.), (20., 15.)]);
            if matches!(glyph, Icon::Import) {
                line(&[(12., 3.), (12., 15.)]);
                line(&[(7., 10.), (12., 15.), (17., 10.)]);
            } else {
                line(&[(12., 16.), (12., 3.)]);
                line(&[(7., 8.), (12., 3.), (17., 8.)]);
            }
        }
        Icon::Replace | Icon::Convert => {
            line(&[(3., 7.), (21., 7.), (17., 3.)]);
            line(&[(21., 17.), (3., 17.), (7., 21.)]);
        }
        Icon::Trash => {
            line(&[(3., 6.), (21., 6.)]);
            line(&[(5., 6.), (6., 21.), (18., 21.), (19., 6.)]);
            box_at(8., 3., 8., 3.);
            line(&[(10., 10.), (10., 17.)]);
            line(&[(14., 10.), (14., 17.)]);
        }
        Icon::Backup => {
            box_at(3., 4., 18., 5.);
            box_at(5., 9., 14., 12.);
            line(&[(9., 13.), (15., 13.)]);
        }
        Icon::Refresh => {
            line(&[
                (4., 9.),
                (7., 4.),
                (15., 3.),
                (21., 8.),
                (21., 15.),
                (16., 21.),
                (8., 21.),
                (4., 17.),
            ]);
            line(&[(3., 3.), (3., 10.), (10., 10.)]);
        }
        Icon::Tools => {
            for y in [6., 12., 18.] {
                line(&[(3., y), (21., y)]);
            }
            for (x, y) in [(8., 6.), (16., 12.), (10., 18.)] {
                p.circle_filled(point(x, y), 2.5 * scale, PAPER);
                p.circle_stroke(point(x, y), 2.5 * scale, stroke);
            }
        }
        Icon::Search => {
            p.circle_stroke(point(10., 10.), 6.5 * scale, stroke);
            line(&[(15., 15.), (21., 21.)]);
        }
        Icon::Check => {
            line(&[(4., 12.), (9., 17.), (20., 6.)]);
        }
        Icon::Log => {
            box_at(4., 3., 16., 18.);
            for y in [8., 12., 16.] {
                line(&[(8., y), (16., y)]);
            }
        }
        Icon::Close => {
            line(&[(6., 6.), (18., 18.)]);
            line(&[(18., 6.), (6., 18.)]);
        }
        Icon::Up => {
            line(&[(12., 21.), (12., 3.)]);
            line(&[(5., 10.), (12., 3.), (19., 10.)]);
        }
    }
}

pub fn toolbar_button(
    ui: &mut Ui,
    glyph: Icon,
    label: &str,
    hint: &str,
    enabled: bool,
    primary: bool,
) -> Response {
    ui.add_enabled_ui(enabled, |ui| {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(76.0, 64.0), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
        });
        let fill = if primary {
            BLUE
        } else if response.hovered() || response.has_focus() {
            PALE
        } else {
            Color32::TRANSPARENT
        };
        let color = if !ui.is_enabled() {
            Color32::from_rgb(157, 166, 183)
        } else if primary {
            Color32::WHITE
        } else {
            INK
        };
        ui.painter().rect_filled(rect, 8, fill);
        if response.has_focus() {
            ui.painter()
                .rect_stroke(rect, 8, Stroke::new(2.0f32, BLUE), StrokeKind::Inside);
        }
        icon(
            ui.painter(),
            Rect::from_center_size(rect.center_top() + egui::vec2(0., 20.), Vec2::splat(23.)),
            glyph,
            color,
        );
        ui.painter().text(
            rect.center_top() + egui::vec2(0., 49.),
            egui::Align2::CENTER_CENTER,
            label,
            FontId::proportional(12.),
            color,
        );
        response.on_hover_text(hint).on_disabled_hover_text(hint)
    })
    .inner
}

pub fn toolbar_menu(ui: &mut Ui, glyph: Icon, label: &str, hint: &str, enabled: bool) -> Response {
    let response = toolbar_button(ui, glyph, label, hint, enabled, false);
    let center = response.rect.right_top() + egui::vec2(-9., 11.);
    ui.painter().line(
        vec![
            center + egui::vec2(-3., -1.5),
            center + egui::vec2(0., 1.5),
            center + egui::vec2(3., -1.5),
        ],
        Stroke::new(1.2f32, if enabled { MUTED } else { BORDER }),
    );
    response
}

pub fn small_button(ui: &mut Ui, glyph: Icon, label: &str) -> Response {
    let width = if label.is_empty() {
        32.0
    } else {
        ui.painter()
            .layout_no_wrap(label.into(), FontId::proportional(13.0), INK)
            .size()
            .x
            + 44.0
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 32.), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    ui.painter().rect_filled(
        rect,
        6,
        if response.hovered() || response.has_focus() {
            PALE
        } else {
            Color32::WHITE
        },
    );
    ui.painter().rect_stroke(
        rect,
        6,
        Stroke::new(1.0f32, if response.has_focus() { BLUE } else { BORDER }),
        StrokeKind::Inside,
    );
    icon(
        ui.painter(),
        Rect::from_min_size(rect.min + egui::vec2(7., 7.), Vec2::splat(18.)),
        glyph,
        if ui.is_enabled() { MUTED } else { BORDER },
    );
    if !label.is_empty() {
        ui.painter().text(
            rect.left_center() + egui::vec2(32., 0.),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::proportional(13.),
            INK,
        );
    }
    response
}

pub fn nav_button(ui: &mut Ui, glyph: Icon, label: &str, active: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(66., 66.), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            active,
            label,
        )
    });
    if active || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(
            rect,
            9,
            if active {
                BLUE
            } else {
                Color32::from_rgb(42, 57, 81)
            },
        );
    }
    let color = if active {
        Color32::WHITE
    } else {
        Color32::from_rgb(174, 190, 214)
    };
    icon(
        ui.painter(),
        Rect::from_center_size(rect.center_top() + egui::vec2(0., 23.), Vec2::splat(23.)),
        glyph,
        color,
    );
    ui.painter().text(
        rect.center_top() + egui::vec2(0., 51.),
        egui::Align2::CENTER_CENTER,
        label,
        FontId::proportional(11.),
        color,
    );
    response
}

pub fn badge(ui: &mut Ui, label: &str, color: Color32, fill: Color32) {
    egui::Frame::new()
        .fill(fill)
        .corner_radius(5)
        .inner_margin(egui::Margin::symmetric(9, 4))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).size(12.).color(color));
        });
}

pub fn empty_art(ui: &mut Ui, glyph: Icon) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(130., 100.), Sense::hover());
    let center = rect.center();
    ui.painter().circle_filled(center, 44., PALE);
    icon(
        ui.painter(),
        Rect::from_center_size(center, Vec2::splat(48.)),
        glyph,
        BLUE,
    );
    for (dx, dy, r) in [(-51., -24., 3.), (51., 12., 4.), (36., -38., 2.)] {
        ui.painter().circle_filled(
            center + egui::vec2(dx, dy),
            r,
            Color32::from_rgb(167, 188, 250),
        );
    }
}

/// Native window/task-switcher icon matching the vector mark in the title bar.
pub fn app_icon() -> egui::IconData {
    let segments: &[(f32, f32, f32, f32)] = &[
        (3., 3., 17., 3.),
        (17., 3., 21., 7.),
        (21., 7., 21., 21.),
        (21., 21., 3., 21.),
        (3., 21., 3., 3.),
        (7., 3., 7., 9.),
        (7., 9., 16., 9.),
        (16., 9., 16., 3.),
        (7., 21., 7., 14.),
        (7., 14., 17., 14.),
        (17., 14., 17., 21.),
    ];
    let mut rgba = Vec::with_capacity(64 * 64 * 4);
    for y in 0..64 {
        for x in 0..64 {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let dx = (px - 32.).abs() - 20.;
            let dy = (py - 32.).abs() - 20.;
            let outside = dx.max(0.).hypot(dy.max(0.)) > 12.;
            let point = egui::vec2((px - 8.) / 2., (py - 8.) / 2.);
            let ink = segments.iter().any(|&(ax, ay, bx, by)| {
                let a = egui::vec2(ax, ay);
                let direction = egui::vec2(bx - ax, by - ay);
                let t = ((point - a).dot(direction) / direction.length_sq()).clamp(0., 1.);
                (point - a - direction * t).length() < 0.8
            });
            rgba.extend_from_slice(&if outside {
                [0, 0, 0, 0]
            } else if ink {
                [255, 255, 255, 255]
            } else {
                [49, 94, 231, 255]
            });
        }
    }
    egui::IconData {
        rgba,
        width: 64,
        height: 64,
    }
}
