use super::KoDrop;
use super::paint::folder_label;
use super::theme::*;
use crate::kindle::valid_folder_name;
use eframe::egui;
use egui::{Color32, Rect, Stroke, vec2};

impl KoDrop {
    pub(super) fn draw_folder_menu(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let mut folders = vec![String::new()];
        folders.extend(self.device.iter().flat_map(|d| d.folders.iter().cloned()));
        let current = self.folder.clone();
        let mut picked = None;
        let mut add_new = false;

        let text_color = if ui.rect_contains_pointer(rect) { ACCENT_DARK } else { INK };
        let selected = egui::RichText::new(folder_label(&current)).font(sans(13.0)).color(text_color);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
            ui.spacing_mut().interact_size.y = rect.height();
            let visuals = ui.visuals_mut();
            for (widget, fill, line) in [
                (&mut visuals.widgets.inactive, FIELD, FIELD_LINE),
                (&mut visuals.widgets.hovered, Color32::WHITE, ACCENT),
                (&mut visuals.widgets.active, DROP_GROUND, ACCENT_DARK),
                (&mut visuals.widgets.open, Color32::WHITE, ACCENT),
            ] {
                widget.weak_bg_fill = fill;
                widget.bg_stroke = Stroke::new(1.0, line);
                widget.corner_radius = 6.0.into();
                widget.expansion = 0.0;
            }

            let combo = egui::ComboBox::from_id_salt("folder")
                .selected_text(selected)
                .width(rect.width())
                .truncate()
                .show_ui(ui, |ui| {
                    let visuals = ui.visuals_mut();
                    visuals.widgets.hovered.weak_bg_fill = DROP_GROUND;
                    visuals.widgets.hovered.bg_stroke = Stroke::NONE;
                    visuals.selection.bg_fill = DROP_PRESSED;
                    visuals.selection.stroke = Stroke::new(1.0, ACCENT_DARK);
                    for folder in &folders {
                        let label = egui::RichText::new(folder_label(folder)).font(sans(13.0));
                        let item = ui
                            .selectable_label(*folder == current, label)
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        if item.clicked() {
                            picked = Some(folder.clone());
                        }
                    }
                    ui.separator();
                    let label = egui::RichText::new("New folder…").font(sans(13.0)).color(ACCENT_DARK);
                    let item = ui
                        .selectable_label(false, label)
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    add_new = item.clicked();
                });
            combo.response.on_hover_cursor(egui::CursorIcon::PointingHand);
        });

        if let Some(folder) = picked {
            self.select_folder(folder);
        }
        if add_new {
            self.new_folder = Some(String::new());
            ui.ctx().request_repaint();
        }
    }

    pub(super) fn draw_new_folder_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut name) = self.new_folder.take() else { return };
        let mut create = false;
        let mut cancel = false;

        let dialog = egui::Modal::new(egui::Id::new("new-folder")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.label(egui::RichText::new("New folder").font(bold(17.0)).color(INK));
            ui.label(egui::RichText::new("It's created inside books on your Kindle").font(sans(13.0)).color(MUTED));

            let visuals = ui.visuals_mut();
            visuals.extreme_bg_color = FIELD;
            visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, FIELD_LINE);
            visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, MUTED);
            let field = egui::TextEdit::singleline(&mut name)
                .font(sans(14.0))
                .desired_width(f32::INFINITY)
                .margin(vec2(8.0, 7.0))
                .hint_text("Folder name");
            ui.add(field).request_focus();

            let check = valid_folder_name(&name);
            let note = match check {
                Err(reason) if !name.trim().is_empty() => reason,
                _ => " ",
            };
            ui.label(egui::RichText::new(note).font(sans(12.0)).color(BAD));
            let valid = check.is_ok();
            create = valid && ui.input(|i| i.key_pressed(egui::Key::Enter));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().button_padding = vec2(14.0, 7.0);
                let visuals = ui.visuals_mut();
                for (widget, fill) in [
                    (&mut visuals.widgets.inactive, ACCENT),
                    (&mut visuals.widgets.hovered, ACCENT_DARK),
                    (&mut visuals.widgets.active, ACCENT_DARK),
                ] {
                    widget.weak_bg_fill = fill;
                    widget.bg_stroke = Stroke::NONE;
                    widget.expansion = 0.0;
                }
                let label = egui::RichText::new("Create").font(sans(13.0)).color(Color32::WHITE);
                let button = ui.add_enabled(valid, egui::Button::new(label));
                create |= button.on_hover_cursor(egui::CursorIcon::PointingHand).clicked();

                let visuals = ui.visuals_mut();
                visuals.widgets.inactive.weak_bg_fill = FIELD;
                visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, FIELD_LINE);
                visuals.widgets.hovered.weak_bg_fill = Color32::WHITE;
                visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT);
                visuals.widgets.active.weak_bg_fill = DROP_GROUND;
                visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_DARK);
                let label = egui::RichText::new("Cancel").font(sans(13.0)).color(INK);
                let button = ui.add(egui::Button::new(label));
                cancel = button.on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
            });
        });

        if create {
            self.start_create_folder(name.trim().to_string(), ctx);
        } else if !cancel && !dialog.should_close() {
            self.new_folder = Some(name);
        }
    }
}
