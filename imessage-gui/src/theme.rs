//! Centralized visual theme and layout constants for the native GUI.

use eframe::egui;

pub mod window {
    pub const INITIAL_WIDTH: f32 = 1200.0;
    pub const INITIAL_HEIGHT: f32 = 800.0;
    pub const MIN_WIDTH: f32 = 920.0;
    pub const MIN_HEIGHT: f32 = 620.0;
}

pub mod layout {
    pub const PANEL_PADDING: f32 = 8.0;
    pub const ROW_GAP: f32 = 4.0;
    pub const COMPACT_ROW_GAP: f32 = 2.0;
    pub const SECTION_GAP: f32 = 10.0;
    pub const PANEL_HEADER_BOTTOM_GAP: f32 = 6.0;
    pub const ADVANCED_SOURCE_TOP_GAP: f32 = 8.0;
    pub const PREVIEW_MIN_HEIGHT: f32 = 180.0;
    pub const EXPORT_CONTROLS_HEIGHT: f32 = 330.0;
    pub const EXPORT_CONTROLS_MIN_VISIBLE_HEIGHT: f32 = 140.0;
    pub const PREVIEW_TABLET_MIN_WIDTH: f32 = 360.0;
    pub const PREVIEW_TABLET_MAX_WIDTH: f32 = 640.0;
    pub const PREVIEW_TABLET_MIN_HEIGHT: f32 = 260.0;
    pub const PREVIEW_TABLET_MAX_HEIGHT: f32 = 460.0;
    pub const PREVIEW_TABLET_ASPECT_RATIO: f32 = 4.0 / 3.0;
    pub const PREVIEW_TABLET_CENTER_PADDING: f32 = 16.0;
    pub const PREVIEW_TABLET_BEZEL: f32 = 12.0;
    pub const PREVIEW_TABLET_TOP_BAR_HEIGHT: f32 = 22.0;
    pub const PREVIEW_TABLET_SCREEN_PADDING: f32 = 10.0;
    pub const PREVIEW_TABLET_CAMERA_SIZE: f32 = 5.0;

    pub const LEFT_PANEL_DEFAULT_WIDTH: f32 = 330.0;
    pub const LEFT_PANEL_MIN_WIDTH: f32 = 240.0;
    pub const LEFT_PANEL_MAX_WIDTH: f32 = 560.0;
    pub const ACTIVITY_LOG_WINDOW_WIDTH: f32 = 320.0;
    pub const ACTIVITY_LOG_WINDOW_HEIGHT: f32 = 420.0;
    pub const ACTIVITY_LOG_WINDOW_MIN_WIDTH: f32 = 260.0;
    pub const ACTIVITY_LOG_WINDOW_MIN_HEIGHT: f32 = 220.0;
    pub const ACTIVITY_LOG_WINDOW_MAX_WIDTH: f32 = 520.0;
    pub const ACTIVITY_LOG_WINDOW_MAX_HEIGHT: f32 = 680.0;
    pub const ACTIVITY_LOG_WINDOW_ATTACH_GAP: f32 = 0.0;
    pub const ACTIVITY_LOG_WINDOW_TOP_OFFSET: f32 = 0.0;

    pub const SOURCE_FIELD_WIDTH: f32 = 380.0;
    pub const PASSWORD_FIELD_WIDTH: f32 = 140.0;
    pub const ADVANCED_SOURCE_FIELD_WIDTH: f32 = 320.0;
    pub const CONVERSATION_SEARCH_WIDTH: f32 = 190.0;
    pub const FIELD_LABEL_WIDTH: f32 = 76.0;
    pub const GROUP_LABEL_WIDTH: f32 = 76.0;
    pub const DATE_FIELD_WIDTH: f32 = 96.0;
    pub const TIME_FIELD_WIDTH: f32 = 70.0;
    pub const PARTICIPANT_FILTER_WIDTH: f32 = 180.0;
    pub const FORMAT_WIDTH: f32 = 110.0;
    pub const ATTACHMENT_MODE_WIDTH: f32 = 220.0;
    pub const NAME_MODE_WIDTH: f32 = 110.0;
    pub const CUSTOM_NAME_WIDTH: f32 = 120.0;
    pub const EXPORT_PATH_WIDTH: f32 = 320.0;
    pub const PROGRESS_BAR_WIDTH: f32 = 260.0;
    pub const MIN_FIELD_WIDTH: f32 = 0.0;
    pub const TAB_BUTTON_MIN_WIDTH: f32 = 104.0;
    pub const CALL_LOG_MIN_COLUMN_WIDTH: f32 = 96.0;
    pub const CALL_LOG_COLUMN_GAP: f32 = 18.0;

    pub const PREVIEW_BUBBLE_MAX_FRACTION: f32 = 0.80;
    pub const PREVIEW_BUBBLE_MIN_WIDTH: f32 = 220.0;
    pub const PREVIEW_BUBBLE_RADIUS: f32 = 10.0;

    pub const ITEM_SPACING_X: f32 = 8.0;
    pub const ITEM_SPACING_Y: f32 = 6.0;
    pub const BUTTON_PADDING_X: f32 = 10.0;
    pub const BUTTON_PADDING_Y: f32 = 3.0;
    pub const BUTTON_MIN_WIDTH: f32 = 88.0;
    pub const SMALL_BUTTON_MIN_WIDTH: f32 = 64.0;
    pub const TEXT_FIELD_MARGIN_X: f32 = 6.0;
    pub const TEXT_FIELD_MARGIN_Y: f32 = 2.0;
    pub const CONTROL_HEIGHT: f32 = 24.0;
    pub const TEXT_FIELD_INNER_HEIGHT: f32 = CONTROL_HEIGHT - (TEXT_FIELD_MARGIN_Y * 2.0);
    pub const MENU_MARGIN_X: f32 = 8.0;
    pub const MENU_MARGIN_Y: f32 = 8.0;
    pub const WINDOW_MARGIN_X: f32 = 10.0;
    pub const WINDOW_MARGIN_Y: f32 = 10.0;
    pub const INDENT_WIDTH: f32 = 16.0;
    pub const INTERACT_WIDTH: f32 = 40.0;
    pub const INTERACT_HEIGHT: f32 = CONTROL_HEIGHT;
    pub const DEFAULT_COMBO_WIDTH: f32 = 160.0;
}

pub mod progress {
    pub const MIN_FRACTION: f32 = 0.0;
    pub const MAX_FRACTION: f32 = 1.0;
}

pub mod timing {
    pub const BUSY_REPAINT_MS: u64 = 120;
}

pub mod ids {
    pub const EXPORT_CONTROLS_SCROLL: &str = "export_controls_scroll";
    pub const CALL_LOGS_SCROLL: &str = "call_logs_scroll";
    pub const CALL_LOGS_GRID: &str = "call_logs_grid";
}

mod palette {
    use eframe::egui;

    pub const APP_BG: egui::Color32 = egui::Color32::from_rgb(228, 234, 242);
    pub const PANEL_BG: egui::Color32 = egui::Color32::from_rgb(246, 248, 251);
    pub const FIELD_BG: egui::Color32 = egui::Color32::WHITE;
    pub const CONTROL_BG: egui::Color32 = egui::Color32::from_rgb(255, 255, 255);
    pub const CONTROL_HOVER_BG: egui::Color32 = egui::Color32::from_rgb(238, 245, 255);
    pub const CONTROL_ACTIVE_BG: egui::Color32 = egui::Color32::from_rgb(220, 236, 255);
    pub const CONTROL_DISABLED_BG: egui::Color32 = egui::Color32::from_rgb(237, 241, 246);
    pub const SUBTLE_BG: egui::Color32 = egui::Color32::from_rgb(224, 230, 238);
    pub const CODE_BG: egui::Color32 = egui::Color32::from_rgb(236, 239, 243);
    pub const BORDER: egui::Color32 = egui::Color32::from_rgb(132, 143, 158);
    pub const BORDER_DISABLED: egui::Color32 = BORDER;
    pub const BORDER_SOFT: egui::Color32 = BORDER;
    pub const ACCENT: egui::Color32 = egui::Color32::from_rgb(0, 102, 204);
    pub const SELECTION_BG: egui::Color32 = egui::Color32::from_rgb(204, 226, 255);
    pub const TEXT: egui::Color32 = egui::Color32::from_rgb(31, 36, 44);
    pub const TEXT_WEAK: egui::Color32 = egui::Color32::from_rgb(83, 93, 106);
    pub const WARNING: egui::Color32 = egui::Color32::from_rgb(171, 99, 0);
    pub const ERROR: egui::Color32 = egui::Color32::from_rgb(178, 45, 45);
    pub const BUBBLE_SENT: egui::Color32 = egui::Color32::from_rgb(0, 105, 217);
    pub const BUBBLE_RECEIVED: egui::Color32 = egui::Color32::from_rgb(230, 233, 237);
    pub const BUBBLE_RECEIVED_TEXT: egui::Color32 = TEXT;
    pub const BUBBLE_SENT_TEXT: egui::Color32 = egui::Color32::WHITE;
    pub const BUBBLE_SENT_META: egui::Color32 = egui::Color32::from_rgb(225, 239, 255);
    pub const BUBBLE_RECEIVED_META: egui::Color32 = TEXT_WEAK;
    pub const TABLET_BEZEL: egui::Color32 = egui::Color32::from_rgb(219, 224, 232);
    pub const TABLET_TOP_BAR: egui::Color32 = egui::Color32::from_rgb(238, 242, 247);
    pub const TABLET_SCREEN: egui::Color32 = egui::Color32::from_rgb(250, 251, 253);
    pub const TABLET_CAMERA: egui::Color32 = egui::Color32::from_rgb(96, 106, 120);
}

mod metric {
    pub const BORDER_WIDTH: f32 = 1.0;
    pub const PANEL_BORDER_WIDTH: f32 = 1.0;
    pub const FOCUS_BORDER_WIDTH: f32 = 1.5;
    pub const WIDGET_RADIUS: f32 = 2.0;
    pub const WIDGET_EXPANSION: f32 = 0.0;
    pub const WINDOW_RADIUS: f32 = 6.0;
    pub const TABLET_RADIUS: f32 = 22.0;
    pub const TABLET_SCREEN_RADIUS: f32 = 12.0;
}

pub fn configure(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();

    visuals.dark_mode = false;
    visuals.override_text_color = Some(palette::TEXT);
    visuals.panel_fill = palette::APP_BG;
    visuals.window_fill = palette::PANEL_BG;
    visuals.extreme_bg_color = palette::FIELD_BG;
    visuals.faint_bg_color = palette::SUBTLE_BG;
    visuals.code_bg_color = palette::CODE_BG;
    visuals.hyperlink_color = palette::ACCENT;
    visuals.warn_fg_color = palette::WARNING;
    visuals.error_fg_color = palette::ERROR;
    visuals.window_stroke = panel_stroke(palette::BORDER_SOFT);
    visuals.window_rounding = egui::Rounding::same(metric::WINDOW_RADIUS);
    visuals.menu_rounding = egui::Rounding::same(metric::WINDOW_RADIUS);
    visuals.button_frame = true;
    visuals.collapsing_header_frame = true;
    visuals.selection.bg_fill = palette::SELECTION_BG;
    visuals.selection.stroke = stroke(palette::ACCENT);

    visuals.widgets.noninteractive.bg_fill = palette::CONTROL_DISABLED_BG;
    visuals.widgets.noninteractive.weak_bg_fill = palette::CONTROL_DISABLED_BG;
    visuals.widgets.noninteractive.bg_stroke = stroke(palette::BORDER_DISABLED);
    visuals.widgets.noninteractive.fg_stroke = stroke(palette::TEXT_WEAK);

    visuals.widgets.inactive.bg_fill = palette::CONTROL_BG;
    visuals.widgets.inactive.weak_bg_fill = palette::CONTROL_BG;
    visuals.widgets.inactive.bg_stroke = stroke(palette::BORDER);
    visuals.widgets.inactive.fg_stroke = stroke(palette::TEXT);

    visuals.widgets.hovered.bg_fill = palette::CONTROL_HOVER_BG;
    visuals.widgets.hovered.weak_bg_fill = palette::CONTROL_HOVER_BG;
    visuals.widgets.hovered.bg_stroke = focus_stroke(palette::BORDER);
    visuals.widgets.hovered.fg_stroke = stroke(palette::TEXT);

    visuals.widgets.active.bg_fill = palette::CONTROL_ACTIVE_BG;
    visuals.widgets.active.weak_bg_fill = palette::CONTROL_ACTIVE_BG;
    visuals.widgets.active.bg_stroke = focus_stroke(palette::BORDER);
    visuals.widgets.active.fg_stroke = stroke(palette::TEXT);
    visuals.widgets.open = visuals.widgets.hovered;

    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        widget.rounding = egui::Rounding::same(metric::WIDGET_RADIUS);
        widget.expansion = metric::WIDGET_EXPANSION;
    }

    let mut style = (*ctx.style()).clone();
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(layout::ITEM_SPACING_X, layout::ITEM_SPACING_Y);
    style.spacing.button_padding = egui::vec2(layout::BUTTON_PADDING_X, layout::BUTTON_PADDING_Y);
    style.spacing.menu_margin =
        egui::Margin::symmetric(layout::MENU_MARGIN_X, layout::MENU_MARGIN_Y);
    style.spacing.window_margin =
        egui::Margin::symmetric(layout::WINDOW_MARGIN_X, layout::WINDOW_MARGIN_Y);
    style.spacing.indent = layout::INDENT_WIDTH;
    style.spacing.interact_size = egui::vec2(layout::INTERACT_WIDTH, layout::INTERACT_HEIGHT);
    style.spacing.combo_width = layout::DEFAULT_COMBO_WIDTH;

    ctx.set_style(style);
}

pub fn panel_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(palette::PANEL_BG)
        .stroke(panel_stroke(palette::BORDER_SOFT))
        .inner_margin(egui::Margin::same(layout::PANEL_PADDING))
}

pub fn content_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(palette::FIELD_BG)
        .stroke(panel_stroke(palette::BORDER_SOFT))
        .inner_margin(egui::Margin::same(layout::PANEL_PADDING))
}

pub fn button(text: impl Into<egui::WidgetText>) -> egui::Button<'static> {
    egui::Button::new(text)
        .frame(true)
        .rounding(egui::Rounding::same(metric::WIDGET_RADIUS))
        .min_size(egui::vec2(layout::BUTTON_MIN_WIDTH, layout::CONTROL_HEIGHT))
}

fn small_button(text: impl Into<egui::WidgetText>) -> egui::Button<'static> {
    egui::Button::new(text)
        .frame(true)
        .rounding(egui::Rounding::same(metric::WIDGET_RADIUS))
        .min_size(egui::vec2(
            layout::SMALL_BUTTON_MIN_WIDTH,
            layout::CONTROL_HEIGHT,
        ))
        .small()
}

pub fn gap(ui: &mut egui::Ui, amount: f32) {
    ui.add_space(amount);
}

pub fn inline_separator(ui: &mut egui::Ui) {
    ui.separator();
}

pub fn panel_header(ui: &mut egui::Ui, title: &str, trailing: impl FnOnce(&mut egui::Ui)) {
    gap(ui, layout::ROW_GAP);
    ui.horizontal(|ui| {
        ui.heading(title);
        trailing(ui);
    });
    inline_separator(ui);
    gap(ui, layout::PANEL_HEADER_BOTTOM_GAP);
}

pub fn group_header(ui: &mut egui::Ui, title: &str) {
    gap(ui, layout::COMPACT_ROW_GAP);
    ui.horizontal(|ui| {
        ui.label(strong_small_text(title));
    });
}

pub fn group_gap(ui: &mut egui::Ui) {
    gap(ui, layout::SECTION_GAP);
}

pub fn control_row(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal_wrapped(add_contents);
}

pub fn fixed_height_area(ui: &mut egui::Ui, height: f32, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), height),
        egui::Layout::top_down(egui::Align::Min),
        add_contents,
    );
}

pub fn scrollable_fixed_height_area(
    ui: &mut egui::Ui,
    height: f32,
    id_salt: &'static str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    fixed_height_area(ui, height, |ui| {
        egui::ScrollArea::vertical()
            .id_salt(id_salt)
            .auto_shrink([false, false])
            .show(ui, add_contents);
    });
}

pub fn preview_export_heights(available_height: f32) -> (f32, f32) {
    let available_height = available_height.max(0.0);
    let min_export_height = layout::EXPORT_CONTROLS_MIN_VISIBLE_HEIGHT.min(available_height);
    let max_export_height = (available_height - layout::PREVIEW_MIN_HEIGHT).max(min_export_height);
    let export_height = layout::EXPORT_CONTROLS_HEIGHT
        .min(max_export_height)
        .max(min_export_height);
    let preview_height = (available_height - export_height).max(0.0);

    (preview_height, export_height)
}

pub fn field_label(ui: &mut egui::Ui, text: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(layout::FIELD_LABEL_WIDTH, layout::INTERACT_HEIGHT),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            ui.label(text);
        },
    );
}

pub fn group_label(ui: &mut egui::Ui, text: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(layout::GROUP_LABEL_WIDTH, layout::INTERACT_HEIGHT),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            if !text.is_empty() {
                ui.label(strong_small_text(text));
            }
        },
    );
}

pub fn add_button(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    let response = ui.add(button(text));
    paint_control_response_border(ui, &response);
    response
}

pub fn add_small_button(ui: &mut egui::Ui, text: impl Into<egui::WidgetText>) -> egui::Response {
    let response = ui.add(small_button(text));
    paint_control_response_border(ui, &response);
    response
}

pub fn add_enabled_button(
    ui: &mut egui::Ui,
    enabled: bool,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    let response = ui.add_enabled(enabled, button(text));
    paint_control_response_border(ui, &response);
    response
}

pub fn add_tab_button(
    ui: &mut egui::Ui,
    selected: bool,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    let fill = if selected {
        palette::SELECTION_BG
    } else {
        palette::CONTROL_BG
    };
    let response = ui.add(button(text).fill(fill).min_size(egui::vec2(
        layout::TAB_BUTTON_MIN_WIDTH,
        layout::CONTROL_HEIGHT,
    )));
    paint_control_response_border(ui, &response);
    response
}

pub fn checkbox(
    ui: &mut egui::Ui,
    checked: &mut bool,
    text: impl Into<egui::WidgetText>,
) -> egui::Response {
    let response = ui.checkbox(checked, text);
    paint_checkbox_border(ui, &response);
    response
}

pub fn paint_dropdown_border(ui: &egui::Ui, response: &egui::Response) {
    paint_control_response_border(ui, response);
}

pub fn add_text_field(
    ui: &mut egui::Ui,
    text: &mut dyn egui::TextBuffer,
    width: f32,
    hint: &'static str,
) -> egui::Response {
    add_configured_text_field(ui, text, width, hint, |edit| edit)
}

pub fn add_password_field(
    ui: &mut egui::Ui,
    text: &mut dyn egui::TextBuffer,
    width: f32,
    hint: &'static str,
    hide: bool,
) -> egui::Response {
    add_configured_text_field(ui, text, width, hint, |edit| edit.password(hide))
}

fn add_configured_text_field(
    ui: &mut egui::Ui,
    text: &mut dyn egui::TextBuffer,
    width: f32,
    hint: &'static str,
    configure: impl FnOnce(egui::TextEdit<'_>) -> egui::TextEdit<'_>,
) -> egui::Response {
    let width = resolved_text_field_width(ui, width);
    let inner = text_field_frame(ui).show(ui, |ui| {
        ui.add_sized(
            [width, layout::TEXT_FIELD_INNER_HEIGHT],
            configure(
                egui::TextEdit::singleline(text)
                    .hint_text(hint)
                    .frame(false)
                    .margin(egui::Margin::ZERO),
            ),
        )
    });
    paint_control_response_border(ui, &inner.response);
    inner.inner
}

fn resolved_text_field_width(ui: &egui::Ui, width: f32) -> f32 {
    if width.is_finite() {
        width
    } else {
        ui.available_width()
    }
    .max(layout::MIN_FIELD_WIDTH)
}

fn text_field_frame(ui: &egui::Ui) -> egui::Frame {
    let fill = if ui.is_enabled() {
        palette::CONTROL_BG
    } else {
        palette::CONTROL_DISABLED_BG
    };

    egui::Frame::none()
        .fill(fill)
        .rounding(egui::Rounding::same(metric::WIDGET_RADIUS))
        .inner_margin(egui::Margin::symmetric(
            layout::TEXT_FIELD_MARGIN_X,
            layout::TEXT_FIELD_MARGIN_Y,
        ))
}

fn paint_checkbox_border(ui: &egui::Ui, response: &egui::Response) {
    if ui.is_rect_visible(response.rect) {
        let (_, checkbox_rect) = ui.spacing().icon_rectangles(response.rect);
        paint_control_rect_border(ui, checkbox_rect, response);
    }
}

fn paint_control_response_border(ui: &egui::Ui, response: &egui::Response) {
    paint_control_rect_border(ui, response.rect, response);
}

fn paint_control_rect_border(ui: &egui::Ui, rect: egui::Rect, response: &egui::Response) {
    if ui.is_rect_visible(rect) {
        ui.painter().rect_stroke(
            rect,
            egui::Rounding::same(metric::WIDGET_RADIUS),
            response_stroke(response),
        );
    }
}

pub fn activity_log_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("activity_log_viewport")
}

pub fn activity_log_viewport_builder(ctx: &egui::Context) -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("Activity log")
        .with_position(activity_log_viewport_position(ctx))
        .with_inner_size(egui::vec2(
            layout::ACTIVITY_LOG_WINDOW_WIDTH,
            layout::ACTIVITY_LOG_WINDOW_HEIGHT,
        ))
        .with_min_inner_size(egui::vec2(
            layout::ACTIVITY_LOG_WINDOW_MIN_WIDTH,
            layout::ACTIVITY_LOG_WINDOW_MIN_HEIGHT,
        ))
        .with_max_inner_size(egui::vec2(
            layout::ACTIVITY_LOG_WINDOW_MAX_WIDTH,
            layout::ACTIVITY_LOG_WINDOW_MAX_HEIGHT,
        ))
        .with_resizable(true)
        .with_decorations(true)
        .with_taskbar(false)
        .with_minimize_button(false)
        .with_maximize_button(false)
}

fn activity_log_viewport_position(ctx: &egui::Context) -> egui::Pos2 {
    ctx.input(|input| {
        if let Some(main_rect) = input.viewport().outer_rect {
            egui::pos2(
                main_rect.max.x + layout::ACTIVITY_LOG_WINDOW_ATTACH_GAP,
                main_rect.min.y + layout::ACTIVITY_LOG_WINDOW_TOP_OFFSET,
            )
        } else {
            egui::pos2(
                window::INITIAL_WIDTH + layout::ACTIVITY_LOG_WINDOW_ATTACH_GAP,
                layout::ACTIVITY_LOG_WINDOW_TOP_OFFSET,
            )
        }
    })
}

pub fn activity_log_viewport_panel(ctx: &egui::Context, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::CentralPanel::default()
        .frame(content_frame())
        .show(ctx, |ui| {
            add_contents(ui);
        });
}

pub fn preview_tablet_area(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    let available = ui.available_size();
    let (area_rect, _) = ui.allocate_exact_size(available, egui::Sense::hover());
    let tablet_size = preview_tablet_size(available);
    if tablet_size.x <= 0.0 || tablet_size.y <= 0.0 {
        return;
    }

    let tablet_rect = egui::Rect::from_center_size(area_rect.center(), tablet_size);
    ui.allocate_new_ui(
        egui::UiBuilder::new()
            .max_rect(tablet_rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        |ui| {
            preview_tablet_frame().show(ui, |ui| {
                ui.set_min_size(tablet_frame_inner_size(tablet_size));
                paint_preview_tablet_top_bar(ui);
                gap(ui, layout::COMPACT_ROW_GAP);
                preview_tablet_screen(ui, add_contents);
            });
        },
    );
}

fn preview_tablet_size(available: egui::Vec2) -> egui::Vec2 {
    let max_width = (available.x - layout::PREVIEW_TABLET_CENTER_PADDING * 2.0)
        .clamp(0.0, layout::PREVIEW_TABLET_MAX_WIDTH);
    let max_height = (available.y - layout::PREVIEW_TABLET_CENTER_PADDING * 2.0)
        .clamp(0.0, layout::PREVIEW_TABLET_MAX_HEIGHT);
    if max_width <= 0.0 || max_height <= 0.0 {
        return egui::Vec2::ZERO;
    }

    let aspect_ratio = layout::PREVIEW_TABLET_ASPECT_RATIO;
    let mut width = max_width;
    let mut height = width / aspect_ratio;
    if height > max_height {
        height = max_height;
        width = height * aspect_ratio;
    }

    let min_width = layout::PREVIEW_TABLET_MIN_WIDTH.min(max_width);
    let min_height = layout::PREVIEW_TABLET_MIN_HEIGHT.min(max_height);
    if width < min_width {
        width = min_width;
        height = width / aspect_ratio;
    }
    if height < min_height {
        height = min_height;
        width = height * aspect_ratio;
    }
    if width > max_width {
        width = max_width;
        height = width / aspect_ratio;
    }
    if height > max_height {
        height = max_height;
        width = height * aspect_ratio;
    }

    egui::vec2(width, height)
}

fn preview_tablet_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(palette::TABLET_BEZEL)
        .stroke(panel_stroke(palette::BORDER_SOFT))
        .rounding(egui::Rounding::same(metric::TABLET_RADIUS))
        .inner_margin(egui::Margin::same(layout::PREVIEW_TABLET_BEZEL))
}

fn tablet_frame_inner_size(tablet_size: egui::Vec2) -> egui::Vec2 {
    egui::vec2(
        (tablet_size.x - layout::PREVIEW_TABLET_BEZEL * 2.0).max(0.0),
        (tablet_size.y - layout::PREVIEW_TABLET_BEZEL * 2.0).max(0.0),
    )
}

fn paint_preview_tablet_top_bar(ui: &mut egui::Ui) {
    let size = egui::vec2(ui.available_width(), layout::PREVIEW_TABLET_TOP_BAR_HEIGHT);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(
        rect,
        egui::Rounding::same(metric::TABLET_SCREEN_RADIUS),
        palette::TABLET_TOP_BAR,
    );
    ui.painter().circle_filled(
        rect.center(),
        layout::PREVIEW_TABLET_CAMERA_SIZE,
        palette::TABLET_CAMERA,
    );
}

fn preview_tablet_screen(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    let size = egui::vec2(ui.available_width(), ui.available_height().max(0.0));
    ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| {
        preview_tablet_screen_frame().show(ui, |ui| {
            ui.set_min_size(tablet_screen_inner_size(size));
            add_contents(ui);
        });
    });
}

fn preview_tablet_screen_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(palette::TABLET_SCREEN)
        .stroke(panel_stroke(palette::BORDER_SOFT))
        .rounding(egui::Rounding::same(metric::TABLET_SCREEN_RADIUS))
        .inner_margin(egui::Margin::same(layout::PREVIEW_TABLET_SCREEN_PADDING))
}

fn tablet_screen_inner_size(screen_size: egui::Vec2) -> egui::Vec2 {
    egui::vec2(
        (screen_size.x - layout::PREVIEW_TABLET_SCREEN_PADDING * 2.0).max(0.0),
        (screen_size.y - layout::PREVIEW_TABLET_SCREEN_PADDING * 2.0).max(0.0),
    )
}

pub fn centered_preview_message(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.allocate_ui_with_layout(
        ui.available_size(),
        egui::Layout::centered_and_justified(egui::Direction::TopDown),
        |ui| {
            ui.label(muted_text(text));
        },
    );
}

fn response_stroke(response: &egui::Response) -> egui::Stroke {
    if response.hovered() || response.has_focus() || response.is_pointer_button_down_on() {
        focus_stroke(palette::BORDER)
    } else {
        stroke(palette::BORDER)
    }
}

pub fn preview_bubble_fill(is_from_me: bool) -> egui::Color32 {
    if is_from_me {
        palette::BUBBLE_SENT
    } else {
        palette::BUBBLE_RECEIVED
    }
}

pub fn preview_bubble_text(is_from_me: bool) -> egui::Color32 {
    if is_from_me {
        palette::BUBBLE_SENT_TEXT
    } else {
        palette::BUBBLE_RECEIVED_TEXT
    }
}

pub fn preview_bubble_meta(is_from_me: bool) -> egui::Color32 {
    if is_from_me {
        palette::BUBBLE_SENT_META
    } else {
        palette::BUBBLE_RECEIVED_META
    }
}

pub fn preview_bubble_frame(is_from_me: bool) -> egui::Frame {
    egui::Frame::none()
        .fill(preview_bubble_fill(is_from_me))
        .rounding(layout::PREVIEW_BUBBLE_RADIUS)
        .inner_margin(egui::Margin::symmetric(
            layout::BUTTON_PADDING_X,
            layout::BUTTON_PADDING_Y,
        ))
}

pub fn muted_text(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).weak()
}

pub fn small_muted_text(text: impl Into<String>) -> egui::RichText {
    muted_text(text).small()
}

pub fn strong_small_text(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).strong().small()
}

pub fn log_text(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).small().monospace()
}

pub fn error_text(text: impl Into<String>) -> egui::RichText {
    egui::RichText::new(text).color(palette::ERROR)
}

pub fn preview_body_text(text: impl Into<String>, is_from_me: bool) -> egui::RichText {
    egui::RichText::new(text).color(preview_bubble_text(is_from_me))
}

pub fn preview_meta_text(text: impl Into<String>, is_from_me: bool) -> egui::RichText {
    egui::RichText::new(text)
        .small()
        .color(preview_bubble_meta(is_from_me))
}

pub fn preview_annotation_text(text: impl Into<String>, is_from_me: bool) -> egui::RichText {
    preview_meta_text(text, is_from_me).italics()
}

fn stroke(color: egui::Color32) -> egui::Stroke {
    egui::Stroke::new(metric::BORDER_WIDTH, color)
}

fn panel_stroke(color: egui::Color32) -> egui::Stroke {
    egui::Stroke::new(metric::PANEL_BORDER_WIDTH, color)
}

fn focus_stroke(color: egui::Color32) -> egui::Stroke {
    egui::Stroke::new(metric::FOCUS_BORDER_WIDTH, color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_widgets_have_resting_borders() {
        let ctx = egui::Context::default();
        configure(&ctx);

        let style = ctx.style();
        let inactive = style.visuals.widgets.inactive;

        assert!(inactive.bg_stroke.width >= metric::BORDER_WIDTH);
        assert_ne!(inactive.bg_stroke.color, egui::Color32::TRANSPARENT);
        assert_ne!(inactive.bg_fill, style.visuals.panel_fill);
    }

    #[test]
    fn disabled_widgets_keep_visible_resting_borders() {
        let ctx = egui::Context::default();
        configure(&ctx);

        let style = ctx.style();
        let disabled = style.visuals.widgets.noninteractive;

        assert!(disabled.bg_stroke.width >= metric::BORDER_WIDTH);
        assert_ne!(disabled.bg_stroke.color, egui::Color32::TRANSPARENT);
        assert_ne!(disabled.bg_fill, style.visuals.panel_fill);
    }

    #[test]
    fn widget_borders_keep_color_and_shape_across_states() {
        let ctx = egui::Context::default();
        configure(&ctx);

        let style = ctx.style();
        let widgets = &style.visuals.widgets;
        let strokes = [
            widgets.noninteractive.bg_stroke,
            widgets.inactive.bg_stroke,
            widgets.hovered.bg_stroke,
            widgets.active.bg_stroke,
            widgets.open.bg_stroke,
        ];

        for stroke in strokes {
            assert_eq!(stroke.color, palette::BORDER);
        }

        assert_eq!(widgets.inactive.bg_stroke.width, metric::BORDER_WIDTH);
        assert_eq!(widgets.noninteractive.bg_stroke.width, metric::BORDER_WIDTH);
        assert_eq!(widgets.hovered.bg_stroke.width, metric::FOCUS_BORDER_WIDTH);
        assert_eq!(widgets.active.bg_stroke.width, metric::FOCUS_BORDER_WIDTH);
        assert_eq!(widgets.open.bg_stroke.width, metric::FOCUS_BORDER_WIDTH);

        assert_eq!(widgets.inactive.rounding, widgets.hovered.rounding);
        assert_eq!(widgets.inactive.rounding, widgets.active.rounding);
        assert_eq!(widgets.inactive.rounding, widgets.open.rounding);

        assert_eq!(widgets.inactive.expansion, metric::WIDGET_EXPANSION);
        assert_eq!(widgets.hovered.expansion, metric::WIDGET_EXPANSION);
        assert_eq!(widgets.active.expansion, metric::WIDGET_EXPANSION);
        assert_eq!(widgets.open.expansion, metric::WIDGET_EXPANSION);
    }

    #[test]
    fn preview_bubbles_keep_readable_sent_and_received_contrast() {
        assert_ne!(preview_bubble_fill(true), preview_bubble_text(true));
        assert_ne!(preview_bubble_fill(false), preview_bubble_text(false));
    }

    #[test]
    fn preview_tablet_size_stays_inside_available_area() {
        let available = egui::vec2(900.0, 520.0);
        let size = preview_tablet_size(available);

        assert!(size.x <= layout::PREVIEW_TABLET_MAX_WIDTH);
        assert!(size.y <= layout::PREVIEW_TABLET_MAX_HEIGHT);
        assert!(size.x <= available.x - layout::PREVIEW_TABLET_CENTER_PADDING * 2.0);
        assert!(size.y <= available.y - layout::PREVIEW_TABLET_CENTER_PADDING * 2.0);
        assert!((size.x / size.y - layout::PREVIEW_TABLET_ASPECT_RATIO).abs() < 0.01);
    }

    #[test]
    fn preview_export_split_keeps_export_scroll_region_visible() {
        let available_height = 760.0;
        let (preview_height, export_height) = preview_export_heights(available_height);

        assert!(preview_height >= layout::PREVIEW_MIN_HEIGHT);
        assert_eq!(export_height, layout::EXPORT_CONTROLS_HEIGHT);
        assert!(preview_height + export_height <= available_height);
    }

    #[test]
    fn preview_export_split_prioritizes_scroll_region_when_cramped() {
        let available_height = 260.0;
        let (preview_height, export_height) = preview_export_heights(available_height);

        assert!(export_height >= layout::EXPORT_CONTROLS_MIN_VISIBLE_HEIGHT);
        assert!(preview_height + export_height <= available_height);
    }
}
