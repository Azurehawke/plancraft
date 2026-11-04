//! The ribbon's buttons, as PlanCraft draws them: the same shapes as the shared ribbon buttons,
//! with the icon in the tool colour the person picked (View > Tool icons).

use egui::{Align2, Rect, Response, Sense, Ui, pos2, vec2};
use gridcraft_ui_egui::i18n;
use gridcraft_ui_egui::icons::{self, Icon};
use gridcraft_ui_egui::theme;

/// Size of the dropdown arrow beside a big button's last label line.
const BIG_ARROW: f32 = 10.0;

fn translate(ui: &Ui, text: &str) -> String {
    i18n::current(ui.ctx()).tr(text).replace('\n', " ")
}

/// Large ribbon button: a 30 px icon over a one- or two-line label.
pub(crate) fn big(ui: &mut Ui, icon: Icon, label: &str, tip: &str, dropdown: bool) -> Response {
    let t = crate::theme::tokens(ui.ctx());
    let label = i18n::current(ui.ctx()).tr(label);
    let font = theme::ui_font(11.5);
    let lines: Vec<_> = label.split('\n').map(|line| ui.painter().layout_no_wrap(line.to_string(), font.clone(), t.text)).collect();
    let arrow_space = if dropdown { 4.0 + BIG_ARROW } else { 0.0 };
    let text_w = lines.iter().enumerate().map(|(i, line)| line.size().x + if i + 1 == lines.len() { arrow_space } else { 0.0 }).fold(0.0, f32::max);
    let w = (text_w + 12.0).max(44.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 72.0), Sense::click());
    if resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 5.0, t.pressed);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 5.0, t.hover);
    }
    icons::paint(
        ui.painter(),
        Rect::from_center_size(pos2(rect.center().x, rect.top() + 21.0), vec2(30.0, 30.0)),
        icon,
        crate::theme::tool_icon(ui.ctx()),
    );
    for (i, line) in lines.iter().enumerate() {
        let has_arrow = dropdown && i + 1 == lines.len();
        let line_w = line.size().x + if has_arrow { arrow_space } else { 0.0 };
        let left = rect.center().x - line_w / 2.0;
        let y = rect.top() + 46.0 + i as f32 * 13.0;
        ui.painter().galley(pos2(left, y - line.size().y / 2.0), line.clone(), t.text);
        if has_arrow {
            icons::paint(
                ui.painter(),
                Rect::from_center_size(pos2(left + line_w - BIG_ARROW / 2.0, y + 1.0), vec2(BIG_ARROW, BIG_ARROW)),
                Icon::Chevron,
                t.text_dim,
            );
        }
    }
    let name = translate(ui, label);
    let enabled = ui.is_enabled();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, &name));
    resp.on_hover_text(translate(ui, tip))
}

/// Small ribbon button: a 16 px icon with an optional label to the right.
pub(crate) fn small(ui: &mut Ui, icon: Icon, label: &str, tip: &str, dropdown: bool) -> Response {
    let t = crate::theme::tokens(ui.ctx());
    let font = theme::ui_font(12.5);
    let label = i18n::current(ui.ctx()).tr(label);
    let tw = if label.is_empty() { 0.0 } else { ui.painter().layout_no_wrap(label.to_string(), font.clone(), t.text).size().x + 6.0 };
    let w = 24.0 + tw + if dropdown { 12.0 } else { 0.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 23.0), Sense::click());
    if resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 4.0, t.pressed);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, t.hover);
    }
    icons::paint(
        ui.painter(),
        Rect::from_center_size(pos2(rect.left() + 12.0, rect.center().y), vec2(16.0, 16.0)),
        icon,
        crate::theme::tool_icon(ui.ctx()),
    );
    if !label.is_empty() {
        ui.painter().text(pos2(rect.left() + 24.0, rect.center().y), Align2::LEFT_CENTER, label, font, t.text);
    }
    if dropdown {
        icons::paint(
            ui.painter(),
            Rect::from_center_size(pos2(rect.right() - 7.0, rect.center().y), vec2(BIG_ARROW * 0.7, BIG_ARROW * 0.7)),
            Icon::Chevron,
            t.text_dim,
        );
    }
    let name = translate(ui, label);
    let enabled = ui.is_enabled();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, &name));
    resp.on_hover_text(translate(ui, tip))
}
