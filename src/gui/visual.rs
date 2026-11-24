use eframe::egui::{self, Color32, FontId, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

pub const BLUE: Color32 = Color32::from_rgb(49, 94, 231);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}
impl Appearance {
    pub fn name(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
    pub fn parse(value: &str) -> Self {
        match value {
            "Light" => Self::Light,
            "Dark" => Self::Dark,
            _ => Self::System,
        }
    }
    pub fn preference(self) -> egui::ThemePreference {
        match self {
            Self::System => egui::ThemePreference::System,
            Self::Light => egui::ThemePreference::Light,
            Self::Dark => egui::ThemePreference::Dark,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Colors {
    pub ink: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub pale: Color32,
    pub border: Color32,
    pub paper: Color32,
    pub surface: Color32,
    pub nav: Color32,
    pub green: Color32,
    pub amber: Color32,
    pub red: Color32,
    pub amber_fill: Color32,
    pub disabled: Color32,
}
impl Colors {
    fn for_theme(theme: egui::Theme) -> Self {
        if theme == egui::Theme::Dark {
            Self {
                ink: Color32::from_rgb(227, 234, 246),
                muted: Color32::from_rgb(161, 175, 199),
                accent: Color32::from_rgb(145, 175, 255),
                pale: Color32::from_rgb(37, 54, 88),
                border: Color32::from_rgb(57, 71, 94),
                paper: Color32::from_rgb(18, 25, 37),
                surface: Color32::from_rgb(26, 35, 50),
                nav: Color32::from_rgb(13, 20, 32),
                green: Color32::from_rgb(104, 210, 172),
                amber: Color32::from_rgb(243, 192, 101),
                red: Color32::from_rgb(255, 140, 150),
                amber_fill: Color32::from_rgb(66, 49, 29),
                disabled: Color32::from_rgb(100, 114, 137),
            }
        } else {
            Self {
                ink: Color32::from_rgb(31, 42, 62),
                muted: Color32::from_rgb(101, 115, 138),
                accent: BLUE,
                pale: Color32::from_rgb(233, 239, 255),
                border: Color32::from_rgb(221, 227, 237),
                paper: Color32::from_rgb(246, 248, 252),
                surface: Color32::WHITE,
                nav: Color32::from_rgb(24, 35, 55),
                green: Color32::from_rgb(28, 124, 98),
                amber: Color32::from_rgb(151, 94, 15),
                red: Color32::from_rgb(187, 58, 73),
                amber_fill: Color32::from_rgb(255, 245, 223),
                disabled: Color32::from_rgb(157, 166, 183),
            }
        }
    }
}
pub fn colors(ctx: &egui::Context) -> Colors {
    Colors::for_theme(ctx.theme())
}

pub fn apply(ctx: &egui::Context) {
    ctx.options_mut(|options| options.fallback_theme = egui::Theme::Light);
    for theme in [egui::Theme::Light, egui::Theme::Dark] {
        let c = Colors::for_theme(theme);
        ctx.style_mut_of(theme, |style| {
            style.visuals = if theme == egui::Theme::Dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            };
            style.visuals.override_text_color = Some(c.ink);
            style.visuals.panel_fill = c.paper;
            style.visuals.window_fill = c.surface;
            style.visuals.window_stroke = Stroke::new(1.0f32, c.border);
            style.visuals.window_corner_radius = 12.into();
            style.visuals.extreme_bg_color = c.surface;
            style.visuals.faint_bg_color = c.paper;
            style.visuals.selection.bg_fill = c.pale;
            style.visuals.selection.stroke = Stroke::new(1.0f32, c.accent);
            style.visuals.hyperlink_color = c.accent;
            style.visuals.warn_fg_color = c.amber;
            style.visuals.error_fg_color = c.red;
            style.visuals.widgets.inactive.bg_fill = c.surface;
            style.visuals.widgets.inactive.weak_bg_fill = c.surface;
            style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0f32, c.border);
            for widget in [
                &mut style.visuals.widgets.hovered,
                &mut style.visuals.widgets.active,
            ] {
                widget.bg_fill = c.pale;
                widget.weak_bg_fill = c.pale;
                widget.bg_stroke = Stroke::new(1.0f32, c.accent);
            }
            for widget in [
                &mut style.visuals.widgets.inactive,
                &mut style.visuals.widgets.hovered,
                &mut style.visuals.widgets.active,
                &mut style.visuals.widgets.noninteractive,
            ] {
                widget.fg_stroke = Stroke::new(1.3f32, c.ink);
                widget.corner_radius = 6.into();
            }
            style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0f32, c.border);
            style.visuals.weak_text_color = Some(c.muted);
            style.spacing.item_spacing = egui::vec2(10., 10.);
            style.spacing.button_padding = egui::vec2(12., 8.);
            style.spacing.interact_size.y = 32.;
            style.spacing.window_margin = 20.into();
            for text in [egui::TextStyle::Body, egui::TextStyle::Button] {
                style.text_styles.insert(text, FontId::proportional(14.));
            }
            style
                .text_styles
                .insert(egui::TextStyle::Small, FontId::proportional(12.));
            style
                .text_styles
                .insert(egui::TextStyle::Heading, FontId::proportional(25.));
        });
    }
}

#[derive(Clone, Copy)]
pub enum Icon {
    Minimize,
    Maximize,
    RestoreWindow,
    Monitor,
    Sun,
    Moon,
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
    Swap,
    Format,
    Back,
    Copy,
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
        Icon::Minimize => {
            line(&[(5., 16.), (19., 16.)]);
        }
        Icon::Maximize => {
            box_at(5., 5., 14., 14.);
        }
        Icon::RestoreWindow => {
            box_at(4., 8., 12., 12.);
            line(&[(8., 8.), (8., 4.), (20., 4.), (20., 16.), (16., 16.)]);
        }
        Icon::Monitor => {
            box_at(3., 4., 18., 13.);
            line(&[(12., 17.), (12., 21.)]);
            line(&[(8., 21.), (16., 21.)]);
        }
        Icon::Sun => {
            p.circle_stroke(point(12., 12.), 4. * scale, stroke);
            for (ax, ay, bx, by) in [
                (12., 1., 12., 4.),
                (12., 20., 12., 23.),
                (1., 12., 4., 12.),
                (20., 12., 23., 12.),
                (4., 4., 6., 6.),
                (18., 18., 20., 20.),
                (4., 20., 6., 18.),
                (18., 6., 20., 4.),
            ] {
                line(&[(ax, ay), (bx, by)]);
            }
        }
        Icon::Moon => {
            line(&[
                (19., 16.),
                (16., 21.),
                (10., 22.),
                (5., 19.),
                (2., 14.),
                (3., 8.),
                (7., 3.),
                (10., 2.),
                (8., 7.),
                (9., 12.),
                (13., 16.),
                (19., 16.),
            ]);
        }
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
                // Keep the badge visible on blue (primary) and dimmed (disabled) buttons.
                let c = colors(p.ctx());
                let (fill, mark) = if color == Color32::WHITE {
                    (Color32::WHITE, BLUE)
                } else if color == c.disabled {
                    (c.disabled, c.surface)
                } else {
                    (BLUE, Color32::WHITE)
                };
                p.circle_filled(point(19., 18.), 6.0 * scale, fill);
                let mark = Stroke::new(1.7 * scale, mark);
                p.line_segment([point(16., 18.), point(22., 18.)], mark);
                p.line_segment([point(19., 15.), point(19., 21.)], mark);
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
        Icon::Convert => {
            line(&[(3., 7.), (21., 7.), (17., 3.)]);
            line(&[(21., 17.), (3., 17.), (7., 21.)]);
        }
        Icon::Replace => {
            // A disk receiving an incoming arrow.
            line(&[
                (3., 9.),
                (13., 9.),
                (16., 12.),
                (16., 21.),
                (3., 21.),
                (3., 9.),
            ]);
            box_at(6., 15., 7., 6.);
            line(&[(10., 3.), (21., 3.), (21., 10.)]);
            line(&[(18., 7.), (21., 10.), (24., 7.)]);
        }
        Icon::Swap => {
            line(&[(8., 20.), (8., 4.)]);
            line(&[(4., 8.), (8., 4.), (12., 8.)]);
            line(&[(16., 4.), (16., 20.)]);
            line(&[(12., 16.), (16., 20.), (20., 16.)]);
        }
        Icon::Format => {
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
            for y in [14., 17.5] {
                line(&[(7., y), (17., y)]);
            }
        }
        Icon::Back => {
            line(&[(21., 12.), (3., 12.)]);
            line(&[(10., 5.), (3., 12.), (10., 19.)]);
        }
        Icon::Copy => {
            box_at(8., 8., 13., 13.);
            line(&[(16., 8.), (16., 3.), (3., 3.), (3., 16.), (8., 16.)]);
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
                p.circle_filled(point(x, y), 2.5 * scale, colors(p.ctx()).surface);
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
        let (rect, response) = ui.allocate_exact_size(egui::vec2(72.0, 64.0), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
        });
        let fill = if primary && !ui.is_enabled() {
            Color32::TRANSPARENT
        } else if primary {
            BLUE
        } else if response.hovered() || response.has_focus() {
            colors(ui.ctx()).pale
        } else {
            Color32::TRANSPARENT
        };
        let color = if !ui.is_enabled() {
            colors(ui.ctx()).disabled
        } else if primary {
            Color32::WHITE
        } else {
            colors(ui.ctx()).ink
        };
        ui.painter().rect_filled(rect, 8, fill);
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                8,
                Stroke::new(2.0f32, colors(ui.ctx()).accent),
                StrokeKind::Inside,
            );
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
    chevron(
        ui.painter(),
        response.rect.right_top() + egui::vec2(-9., 11.),
        if enabled {
            colors(ui.ctx()).muted
        } else {
            colors(ui.ctx()).disabled
        },
    );
    response
}

pub fn small_button(ui: &mut Ui, glyph: Icon, label: &str) -> Response {
    framed_button(ui, Some(glyph), label, false)
}

/// A framed secondary button without an icon, matching `small_button`.
pub fn text_button(ui: &mut Ui, label: &str) -> Response {
    framed_button(ui, None, label, false)
}

/// A `small_button` that opens a menu, marked with a drawn chevron (the
/// default fonts have no reliable down-triangle glyph).
pub fn dropdown_button(ui: &mut Ui, glyph: Icon, label: &str) -> Response {
    framed_button(ui, Some(glyph), label, true)
}

fn chevron(p: &egui::Painter, center: egui::Pos2, color: Color32) {
    p.line(
        vec![
            center + egui::vec2(-3.5, -1.5),
            center + egui::vec2(0., 2.),
            center + egui::vec2(3.5, -1.5),
        ],
        Stroke::new(1.4f32, color),
    );
}

/// Standard single-line field: same height and padding as the buttons.
pub fn text_field(edit: egui::TextEdit<'_>) -> egui::TextEdit<'_> {
    edit.margin(egui::Margin::symmetric(10, 7))
        .min_size(egui::vec2(0., 32.))
        .vertical_align(egui::Align::Center)
}

fn framed_button(ui: &mut Ui, glyph: Option<Icon>, label: &str, menu: bool) -> Response {
    let c = colors(ui.ctx());
    let text_width = if label.is_empty() {
        0.0
    } else {
        ui.painter()
            .layout_no_wrap(label.into(), FontId::proportional(13.0), c.ink)
            .size()
            .x
    };
    let width = match (glyph.is_some(), label.is_empty()) {
        (true, true) => 32.0,
        (true, false) => text_width + 44.0,
        (false, _) => text_width + 24.0,
    } + if menu { 16.0 } else { 0.0 };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 32.), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    let enabled = ui.is_enabled();
    let active = enabled && (response.hovered() || response.has_focus());
    ui.painter()
        .rect_filled(rect, 6, if active { c.pale } else { c.surface });
    ui.painter().rect_stroke(
        rect,
        6,
        Stroke::new(1.0f32, if active { c.accent } else { c.border }),
        StrokeKind::Inside,
    );
    if let Some(glyph) = glyph {
        icon(
            ui.painter(),
            Rect::from_min_size(rect.min + egui::vec2(7., 7.), Vec2::splat(18.)),
            glyph,
            if enabled { c.muted } else { c.disabled },
        );
    }
    if !label.is_empty() {
        let (pos, align) = if glyph.is_some() {
            (
                rect.left_center() + egui::vec2(32., 0.),
                egui::Align2::LEFT_CENTER,
            )
        } else {
            (rect.center(), egui::Align2::CENTER_CENTER)
        };
        ui.painter().text(
            pos,
            align,
            label,
            FontId::proportional(13.),
            if enabled { c.ink } else { c.disabled },
        );
    }
    if menu {
        chevron(
            ui.painter(),
            rect.right_center() - egui::vec2(14., 0.),
            if enabled { c.muted } else { c.disabled },
        );
    }
    response
}

/// The single emphasized action of a dialog.
pub fn primary_button(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    let c = colors(ui.ctx());
    let (fill, text) = if enabled {
        (BLUE, Color32::WHITE)
    } else {
        (c.pale, c.disabled)
    };
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(label).size(13.).strong().color(text))
            .fill(fill)
            .stroke(Stroke::NONE)
            .corner_radius(6)
            .min_size(egui::vec2(0., 32.)),
    )
}

/// A destructive confirmation button (erase, overwrite).
pub fn danger_button(ui: &mut Ui, label: &str) -> Response {
    let c = colors(ui.ctx());
    ui.add(
        egui::Button::new(
            egui::RichText::new(label)
                .size(13.)
                .strong()
                .color(Color32::WHITE),
        )
        .fill(if ui.visuals().dark_mode {
            Color32::from_rgb(176, 52, 68)
        } else {
            c.red
        })
        .stroke(Stroke::NONE)
        .corner_radius(6)
        .min_size(egui::vec2(0., 32.)),
    )
}

/// A small sort-direction triangle drawn after a column header.
pub fn sort_arrow(ui: &mut Ui, descending: bool) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10., 10.), Sense::hover());
    let c = rect.center();
    let points = if descending {
        vec![
            c + egui::vec2(-4., -2.),
            c + egui::vec2(4., -2.),
            c + egui::vec2(0., 3.),
        ]
    } else {
        vec![
            c + egui::vec2(-4., 2.),
            c + egui::vec2(4., 2.),
            c + egui::vec2(0., -3.),
        ]
    };
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        colors(ui.ctx()).accent,
        Stroke::NONE,
    ));
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
    let galley = ui
        .painter()
        .layout_no_wrap(label.into(), FontId::proportional(11.5), color);
    let (rect, _) = ui.allocate_exact_size(galley.size() + egui::vec2(18., 10.), Sense::hover());
    ui.painter().rect_filled(rect, 5, fill);
    ui.painter()
        .galley(rect.center() - galley.size() / 2., galley, color);
}

pub fn empty_art(ui: &mut Ui, glyph: Icon) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(130., 100.), Sense::hover());
    let center = rect.center();
    ui.painter()
        .circle_filled(center, 44., colors(ui.ctx()).pale);
    icon(
        ui.painter(),
        Rect::from_center_size(center, Vec2::splat(48.)),
        glyph,
        colors(ui.ctx()).accent,
    );
    let dots = colors(ui.ctx()).accent.gamma_multiply(0.45);
    for (dx, dy, r) in [(-51., -24., 3.), (51., 12., 4.), (36., -38., 2.)] {
        ui.painter()
            .circle_filled(center + egui::vec2(dx, dy), r, dots);
    }
}

/// Native window/task-switcher icon matching the vector mark in the title bar.
pub fn app_icon() -> egui::IconData {
    let image = image::load_from_memory(include_bytes!("../../assets/rustgotek.png"))
        .expect("embedded app icon is a valid PNG")
        .into_rgba8();
    let (width, height) = image.dimensions();
    egui::IconData {
        rgba: image.into_raw(),
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn app_icon_is_valid_rgba() {
        let icon = app_icon();
        assert_eq!(icon.width, 512);
        assert_eq!(icon.height, 512);
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
    }

    #[test]
    fn system_theme_changes_and_manual_overrides_update_all_surfaces() {
        let ctx = egui::Context::default();
        apply(&ctx);
        for (preference, system, expected) in [
            (Appearance::System, egui::Theme::Dark, egui::Theme::Dark),
            (Appearance::System, egui::Theme::Light, egui::Theme::Light),
            (Appearance::Dark, egui::Theme::Light, egui::Theme::Dark),
            (Appearance::Light, egui::Theme::Dark, egui::Theme::Light),
        ] {
            ctx.set_theme(preference.preference());
            let _ = ctx.run(
                egui::RawInput {
                    system_theme: Some(system),
                    ..Default::default()
                },
                |ctx| {
                    assert_eq!(ctx.theme(), expected);
                    let c = colors(ctx);
                    assert_eq!(ctx.style().visuals.window_fill, c.surface);
                    assert_eq!(ctx.style().visuals.panel_fill, c.paper);
                    assert_eq!(ctx.style().visuals.override_text_color, Some(c.ink));
                    assert_ne!(c.ink, c.surface);
                },
            );
        }
        for choice in [Appearance::System, Appearance::Dark, Appearance::Light] {
            assert_eq!(Appearance::parse(choice.name()), choice);
        }
        assert_eq!(Appearance::parse("invalid"), Appearance::System);
    }
}
