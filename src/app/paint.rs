use super::theme::{INK, serif};
use crate::kindle::{Book, Progress};
use eframe::egui;
use egui::epaint::TextShape;
use egui::text::{LayoutJob, TextFormat, TextWrapping};
use egui::{Color32, FontId, Pos2, Rect, Shape, Stroke, pos2, vec2};

const COVERS: &[(Color32, Color32)] = &[
    (Color32::from_rgb(0x7A, 0x2E, 0x22), Color32::WHITE),
    (Color32::from_rgb(0xE2, 0xC6, 0x44), INK),
    (Color32::from_rgb(0x1F, 0x3A, 0x5F), Color32::WHITE),
    (Color32::from_rgb(0x27, 0x59, 0x4C), Color32::WHITE),
    (Color32::from_rgb(0x5A, 0x3B, 0x73), Color32::WHITE),
    (Color32::from_rgb(0xB9, 0x55, 0x2B), Color32::WHITE),
];

pub(super) fn folder_label(folder: &str) -> &str {
    if folder.is_empty() { "books" } else { folder }
}

pub(super) fn format_size(bytes: u64) -> String {
    match bytes {
        0..=999_999 => format!("{} KB", bytes / 1000),
        1_000_000..=999_999_999 => format!("{:.1} MB", bytes as f64 / 1e6),
        _ => format!("{:.1} GB", bytes as f64 / 1e9),
    }
}

pub(super) fn format_free(kb: u64) -> String {
    let bytes = kb * 1024;
    if bytes < 1_000_000_000 {
        format!("{} MB free", bytes / 1_000_000)
    } else {
        format!("{:.1} GB free", bytes as f64 / 1e9)
    }
}

pub(super) fn progress_label(progress: &Progress) -> String {
    match progress {
        Progress::NotOpened => "Not opened".to_string(),
        Progress::Unknown => "Progress unknown".to_string(),
        Progress::Read(p) if *p >= 1.0 => "Finished".to_string(),
        Progress::Read(p) => format!("{:.0}% read", p * 100.0),
    }
}

// Draws text from its top-left corner, wrapped to width and cut off after max_rows
pub(super) fn draw_text(
    painter: &egui::Painter,
    pos: Pos2,
    text: &str,
    font: FontId,
    color: Color32,
    width: f32,
    max_rows: usize,
) -> f32 {
    let galley = painter.layout_job(text_job(text, font, color, width, max_rows));
    let height = galley.size().y;
    painter.galley(pos, galley, color);
    height
}

fn text_job(text: &str, font: FontId, color: Color32, width: f32, max_rows: usize) -> LayoutJob {
    let mut job = LayoutJob::single_section(text.to_string(), TextFormat::simple(font, color));
    job.wrap = TextWrapping {
        max_width: width,
        max_rows,
        break_anywhere: false,
        overflow_character: Some('…'),
    };
    job
}

pub(super) fn draw_right_aligned(painter: &egui::Painter, right_top: Pos2, text: &str, font: FontId, color: Color32) {
    painter.text(right_top, egui::Align2::RIGHT_TOP, text, font, color);
}

pub(super) fn draw_dashed_rect(painter: &egui::Painter, rect: Rect, stroke: Stroke) {
    let points = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
        rect.left_top(),
    ];
    painter.add(Shape::dashed_line(&points, stroke, 8.0, 6.0));
}

pub(super) fn draw_bar(painter: &egui::Painter, rect: Rect, fraction: f32, track: Color32, fill: Color32) {
    painter.rect_filled(rect, rect.height() / 2.0, track);
    if fraction > 0.0 {
        let mut filled = rect;
        filled.set_width((rect.width() * fraction.min(1.0)).max(rect.height()));
        painter.rect_filled(filled, rect.height() / 2.0, fill);
    }
}

pub(super) fn draw_cover(painter: &egui::Painter, rect: Rect, book: &Book, index: usize) {
    let (title, author) = (&book.title, &book.author);
    let (background, foreground) = COVERS[index % COVERS.len()];
    painter.rect_filled(rect, 3.0, background);
    let inner = rect.shrink2(vec2(10.0, 14.0));
    draw_text(painter, inner.left_top(), title, serif(15.0), foreground, inner.width(), 5);
    let galley = painter.layout_job(text_job(author, serif(12.0), foreground, inner.width(), 2));
    let pos = pos2(inner.left(), inner.bottom() - galley.size().y);
    painter.galley(pos, galley, foreground);
}

pub(super) fn draw_centered(painter: &egui::Painter, top_center: Pos2, text: &str, font: FontId, color: Color32, width: f32) {
    let galley = painter.layout_job(text_job(text, font, color, width, 2));
    let pos = top_center - vec2(galley.size().x / 2.0, 0.0);
    painter.galley(pos, galley, color);
}

pub(super) fn draw_ghost_cover(painter: &egui::Painter, center: Pos2, title: &str, author: &str) {
    const ANGLE: f32 = -0.07;
    const SIZE: egui::Vec2 = egui::vec2(156.0, 200.0);
    let turn = egui::emath::Rot2::from_angle(ANGLE);
    let corner = |x: f32, y: f32| center + turn * vec2(x, y);
    let (hw, hh) = (SIZE.x / 2.0, SIZE.y / 2.0);
    let (background, foreground) = COVERS[COVERS.len() - 1];
    painter.add(Shape::convex_polygon(
        vec![corner(-hw, -hh), corner(hw, -hh), corner(hw, hh), corner(-hw, hh)],
        background,
        Stroke::NONE,
    ));

    let width = SIZE.x - 20.0;
    let title_galley = painter.layout_job(text_job(title, serif(15.0), foreground, width, 5));
    painter.add(Shape::Text(
        TextShape::new(corner(10.0 - hw, 14.0 - hh), title_galley, foreground).with_angle(ANGLE),
    ));
    let author_galley = painter.layout_job(text_job(author, serif(12.0), foreground, width, 2));
    let author_top = hh - 14.0 - author_galley.size().y;
    painter.add(Shape::Text(
        TextShape::new(corner(10.0 - hw, author_top), author_galley, foreground).with_angle(ANGLE),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_labels() {
        assert_eq!(format_size(2413228), "2.4 MB");
        assert_eq!(format_size(538870), "538 KB");
        assert_eq!(format_free(781088), "799 MB free");
        assert_eq!(progress_label(&Progress::Read(0.23705)), "24% read");
        assert_eq!(progress_label(&Progress::Read(1.0)), "Finished");
        assert_eq!(progress_label(&Progress::NotOpened), "Not opened");
    }
}
