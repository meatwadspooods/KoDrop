use super::paint::{draw_bar, draw_right_aligned, draw_text, folder_label, format_free};
use super::theme::*;
use super::{Connection, KoDrop};
use crate::settings::save_setting;
use eframe::egui;
use egui::{Color32, Rect, Stroke, pos2, vec2};

impl KoDrop {
    fn draw_kindle(&self, painter: &egui::Painter, rect: Rect, incoming: Option<&str>) {
        painter.rect_filled(rect, 14.0, BEZEL);
        let screen = rect.shrink(12.0);
        painter.rect_filled(screen, 0.0, EINK);
        let inner = screen.shrink(10.0);

        let battery = self
            .device
            .as_ref()
            .and_then(|d| d.battery)
            .map_or(String::new(), |b| format!("{b}%"));
        let leaf = folder_label(&self.folder).rsplit('/').next().unwrap_or("books");
        draw_text(painter, inner.left_top(), leaf, mono(10.0), INK, inner.width() - 32.0, 1);
        draw_right_aligned(painter, inner.right_top(), &battery, mono(10.0), INK);
        let mut y = inner.top() + 17.0;
        painter.hline(inner.x_range(), y, Stroke::new(1.0, INK));
        y += 7.0;

        if let Some(title) = incoming {
            let height = 22.0;
            let row = Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width(), height));
            painter.rect_filled(row, 0.0, ACCENT);
            let text_pos = pos2(row.left() + 6.0, y + 4.0);
            draw_text(painter, text_pos, title, serif(10.5), Color32::WHITE, row.width() - 12.0, 1);
            y += height + 7.0;
        }

        for book in self.visible_books() {
            if y + 15.0 > inner.bottom() {
                break;
            }
            draw_text(painter, pos2(inner.left(), y), &book.title, serif(10.5), INK, inner.width(), 1);
            y += 19.0;
            painter.hline(inner.x_range(), y, Stroke::new(1.0, EINK_LINE));
            y += 7.0;
        }
    }

    pub(super) fn draw_device_panel(&mut self, ui: &mut egui::Ui, rect: Rect, incoming: Option<&str>) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, PANEL);
        painter.vline(rect.right(), rect.y_range(), Stroke::new(1.0, LINE));

        let inner = rect.shrink(24.0);
        let kindle = Rect::from_center_size(
            pos2(inner.center().x, inner.top() + 118.0),
            vec2(176.0, 236.0),
        );
        self.draw_kindle(&painter, kindle, incoming);

        let mut y = kindle.bottom() + 18.0;
        y += draw_text(&painter, pos2(inner.left(), y), "Kindle", bold(17.0), INK, inner.width(), 1);
        y += 6.0;
        let (label, color, reason) = match &self.connection {
            Connection::Connecting => ("Connecting…", MUTED, None),
            Connection::Connected => ("Connected", GOOD, None),
            Connection::Failed(reason) => ("Not connected", BAD, Some(reason.as_str())),
        };
        painter.circle_filled(pos2(inner.left() + 4.0, y + 9.0), 4.0, color);
        y += draw_text(&painter, pos2(inner.left() + 15.0, y), label, bold(13.0), color, inner.width(), 1);
        if let Some(reason) = reason {
            y += 4.0;
            y += draw_text(&painter, pos2(inner.left(), y), reason, sans(12.0), MUTED, inner.width(), 3);
        }
        y += 16.0;

        let device = self.device.as_ref();
        let free = device.map_or("Unknown".to_string(), |d| format_free(d.free_kb));
        let used = device.map_or(0.0, |d| {
            d.used_kb as f32 / (d.used_kb + d.free_kb).max(1) as f32
        });
        let row = |y: f32, label: &str, value: &str| {
            painter.text(pos2(inner.left(), y), egui::Align2::LEFT_TOP, label, sans(13.0), INK);
            draw_right_aligned(&painter, pos2(inner.right(), y + 1.0), value, mono(12.0), INK);
        };
        row(y, "Storage", &free);
        y += 25.0;
        let bar = Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width(), 6.0));
        draw_bar(&painter, bar, used, TRACK, INK);
        y += 22.0;
        let battery = device
            .and_then(|d| d.battery)
            .map_or("Unknown".to_string(), |b| format!("{b}%"));
        row(y, "Battery", &battery);
        y += 28.0;
        let koreader = device
            .map(|d| d.koreader.as_str())
            .filter(|v| !v.is_empty())
            .unwrap_or("Unknown");
        row(y, "KOReader", koreader);
        y += 28.0;

        painter.text(pos2(inner.left(), y + 5.0), egui::Align2::LEFT_TOP, "Address", sans(13.0), INK);
        let field_rect = Rect::from_min_size(pos2(inner.right() - 112.0, y), vec2(112.0, 28.0));
        let field = egui::TextEdit::singleline(&mut self.host)
            .font(mono(12.0))
            .horizontal_align(egui::Align::RIGHT)
            .vertical_align(egui::Align::Center)
            .hint_text("10.0.0.0");
        let response = ui
            .scope(|ui| {
                let visuals = ui.visuals_mut();
                visuals.extreme_bg_color = FIELD;
                visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, FIELD_LINE);
                visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, MUTED);
                ui.put(field_rect, field)
            })
            .inner
            .on_hover_text("Your Kindle's IP address");
        if response.changed() {
            save_setting("host", self.host.trim());
        }
        if response.lost_focus() {
            let ctx = ui.ctx().clone();
            self.refresh(&ctx);
        }
    }
}
