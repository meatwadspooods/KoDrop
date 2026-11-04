use super::paint::{draw_bar, draw_centered, draw_cover, draw_dashed_rect, draw_ghost_cover, draw_right_aligned, draw_text, folder_label, format_size, progress_label};
use super::theme::*;
use super::{KoDrop, Status};
use crate::kindle::{Progress, is_book, title_and_author};
use eframe::egui;
use egui::{Color32, Rect, Stroke, pos2, vec2};
use std::path::PathBuf;

// Keeps a lifted cover and its shadow from being clipped by the scroll area
const SHELF_MARGIN: f32 = 12.0;

impl KoDrop {
    fn draw_activity(&self, ui: &egui::Ui, rect: Rect) {
        let painter = ui.painter_at(rect);
        let mut y = rect.top();
        for item in self.recent_items() {
            let (dot, text) = match &item.status {
                Status::Sending => (ACCENT, format!("Sending {} to {}…", item.name, folder_label(&item.folder))),
                Status::Sent => (GOOD, format!("Sent {} to {}", item.name, folder_label(&item.folder))),
                Status::Failed(reason) => (BAD, format!("{}: {reason}", item.name)),
            };
            painter.circle_filled(pos2(rect.left() + 4.0, y + 9.0), 4.0, dot);
            y += draw_text(&painter, pos2(rect.left() + 15.0, y), &text, sans(13.0), INK, rect.width() - 15.0, 2);
            y += 6.0;
        }
    }

    pub(super) fn draw_shelf(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let inner = Rect::from_min_max(
            rect.min + vec2(28.0, 24.0),
            rect.max - vec2(28.0, 24.0),
        );
        let painter = ui.painter_at(rect);

        painter.text(inner.left_top(), egui::Align2::LEFT_TOP, "Books", bold(22.0), INK);
        let count = self.visible_books().len();
        let summary = match (&self.device, count) {
            (None, _) => "Not connected".to_string(),
            (_, 1) => "1 book".to_string(),
            _ => format!("{count} books"),
        };
        painter.text(pos2(inner.left() + 76.0, inner.top() + 9.0), egui::Align2::LEFT_TOP, summary, sans(13.0), MUTED);

        let button_rect = Rect::from_min_size(pos2(inner.right() - 78.0, inner.top()), vec2(78.0, 32.0));
        let label = if self.refreshing { "Refreshing" } else { "Refresh" };
        let hovered = ui.rect_contains_pointer(button_rect);
        let pressed = hovered && ui.input(|i| i.pointer.primary_down());
        let (fill, line, text) = match (hovered, pressed) {
            (true, true) => (DROP_GROUND, ACCENT_DARK, ACCENT_DARK),
            (true, false) => (Color32::WHITE, ACCENT, ACCENT_DARK),
            _ => (FIELD, FIELD_LINE, INK),
        };
        let button = egui::Button::new(egui::RichText::new(label).font(sans(13.0)).color(text))
            .fill(fill)
            .stroke(Stroke::new(1.0, line))
            .corner_radius(6.0);
        let response = ui.put(button_rect, button).on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.clicked() {
            let ctx = ui.ctx().clone();
            self.refresh(&ctx);
        }

        let combo_rect = Rect::from_min_size(pos2(button_rect.left() - 152.0, inner.top()), vec2(140.0, 32.0));
        draw_right_aligned(&painter, pos2(combo_rect.left() - 8.0, inner.top() + 8.0), "Folder", sans(13.0), MUTED);
        self.draw_folder_menu(ui, combo_rect);

        let activity_height = self.recent_items().count() as f32 * 28.0;
        let shelf = Rect::from_min_max(
            pos2(inner.left() - SHELF_MARGIN, inner.top() + 50.0 - SHELF_MARGIN),
            pos2(inner.right() + SHELF_MARGIN, inner.bottom() - activity_height),
        );
        if activity_height > 0.0 {
            let activity = Rect::from_min_max(pos2(inner.left(), shelf.bottom() + 8.0), inner.right_bottom());
            self.draw_activity(ui, activity);
        }

        let choose = ui
            .scope_builder(egui::UiBuilder::new().max_rect(shelf), |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| self.draw_grid(ui, inner.width()))
                    .inner
            })
            .inner;
        if choose {
            let ctx = ui.ctx().clone();
            self.open_picker(&ctx);
        }
    }

    // Returns true when the drop tile is clicked
    fn draw_grid(&self, ui: &mut egui::Ui, width: f32) -> bool {
        const GAP: f32 = 20.0;
        const COVER_HEIGHT: f32 = 200.0;
        const ROW_HEIGHT: f32 = COVER_HEIGHT + 42.0;
        const MIN_CARD_WIDTH: f32 = 140.0;
        let columns = (((width + GAP) / (MIN_CARD_WIDTH + GAP)).floor() as usize).max(1);
        let card_width = (width - GAP * (columns - 1) as f32) / columns as f32;
        let books = self.visible_books();

        let leftover = columns - books.len() % columns;
        let tile_on_new_row = leftover == columns && !books.is_empty();
        let rows = books.len().div_ceil(columns) + usize::from(tile_on_new_row || books.is_empty());
        let height = rows as f32 * (ROW_HEIGHT + GAP);
        let size = vec2(width + SHELF_MARGIN, height + SHELF_MARGIN);
        let (area, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        let painter = ui.painter();

        let cell = |index: usize| {
            pos2(
                area.left() + SHELF_MARGIN + (index % columns) as f32 * (card_width + GAP),
                area.top() + SHELF_MARGIN + (index / columns) as f32 * (ROW_HEIGHT + GAP),
            )
        };
        for (index, book) in books.iter().enumerate() {
            let top_left = cell(index);
            let card = Rect::from_min_size(top_left, vec2(card_width, ROW_HEIGHT));
            let response = ui
                .interact(card, ui.id().with(&book.file), egui::Sense::hover())
                .on_hover_text(&book.file);
            let lift = ui.ctx().animate_bool_with_time(response.id, response.hovered(), 0.12);

            let resting = Rect::from_min_size(top_left, vec2(card_width, COVER_HEIGHT));
            let cover = resting.translate(vec2(0.0, -5.0 * lift));
            if lift > 0.0 {
                let shadow = egui::Shadow {
                    offset: [0, 8],
                    blur: 18,
                    spread: 0,
                    color: Color32::from_black_alpha((70.0 * lift) as u8),
                };
                painter.add(shadow.as_shape(cover, 3.0));
            }
            draw_cover(painter, cover, book, index);

            let fraction = match book.progress {
                Progress::Read(p) => p,
                _ => 0.0,
            };
            let bar = Rect::from_min_size(pos2(resting.left(), resting.bottom() + 8.0), vec2(card_width, 4.0));
            draw_bar(painter, bar, fraction, LINE, ACCENT);

            let y = bar.bottom() + 8.0;
            let size = format_size(book.size);
            let label_color = if response.hovered() { INK } else { MUTED };
            painter.text(pos2(resting.left(), y), egui::Align2::LEFT_TOP, progress_label(&book.progress), sans(12.0), label_color);
            draw_right_aligned(painter, pos2(resting.right(), y), &size, sans(12.0), label_color);
        }

        let span = if books.is_empty() || tile_on_new_row { columns } else { leftover };
        let tile = Rect::from_min_size(
            cell(books.len()),
            vec2(span as f32 * (card_width + GAP) - GAP, COVER_HEIGHT),
        );
        let response = ui
            .interact(tile, ui.id().with("choose-books"), egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Choose books to send")
        });
        let (hovered, pressed) = (response.hovered(), response.is_pointer_button_down_on());
        if pressed {
            painter.rect_filled(tile, 0.0, DROP_PRESSED);
        } else if hovered {
            painter.rect_filled(tile, 0.0, DROP_GROUND);
        }
        let (line, heading, detail) = if hovered {
            (ACCENT, ACCENT_DARK, ACCENT_DARK)
        } else {
            (DASH, INK, MUTED)
        };
        draw_dashed_rect(painter, tile, Stroke::new(2.0, line));
        let center = tile.center();
        painter.text(center - vec2(0.0, 11.0), egui::Align2::CENTER_CENTER, "Drop books anywhere in this window", bold(15.0), heading);
        painter.text(center + vec2(0.0, 12.0), egui::Align2::CENTER_CENTER, "or click to choose them from your Mac", sans(13.0), detail);
        response.clicked()
    }

    pub(super) fn draw_drop_target(&self, ui: &egui::Ui, rect: Rect, hovered: &[PathBuf]) {
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, DROP_GROUND);
        draw_dashed_rect(&painter, rect.shrink(16.0), Stroke::new(3.0, ACCENT));

        let books: Vec<&PathBuf> = hovered.iter().filter(|p| is_book(p)).collect();
        let center = rect.center();
        // Finder only names dragged files on some drags, so an empty list still means "drop here"
        if !hovered.is_empty() && books.is_empty() {
            painter.text(center - vec2(0.0, 14.0), egui::Align2::CENTER_CENTER, "That isn't a book file", bold(22.0), BAD);
            painter.text(center + vec2(0.0, 16.0), egui::Align2::CENTER_CENTER, "KoDrop only sends files KOReader can open, like EPUB, PDF and MOBI", sans(14.0), MUTED);
            return;
        }

        let file_name = |path: &PathBuf| {
            path.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned())
        };
        let (title, author) = books
            .first()
            .map_or((String::new(), String::new()), |path| title_and_author(&file_name(path)));
        if !title.is_empty() {
            draw_ghost_cover(&painter, center - vec2(0.0, 100.0), &title, &author);
        }

        let folder = folder_label(&self.folder);
        let detail = match books.as_slice() {
            [] => format!("It goes to the {folder} folder"),
            [only] => format!("{} goes to the {folder} folder", file_name(only)),
            many => format!("{} books go to the {folder} folder", many.len()),
        };
        painter.text(center + vec2(0.0, 40.0), egui::Align2::CENTER_CENTER, "Release to send to your Kindle", bold(22.0), ACCENT_DARK);
        draw_centered(&painter, center + vec2(0.0, 66.0), &detail, sans(14.0), MUTED, rect.width() - 80.0);
    }
}
