mod app;
mod kindle;
mod settings;

use app::KoDrop;
use eframe::egui;
use std::fs;
use std::path::Path;

// eframe replaces the bundle icon in the Dock with its own default unless it's given one.
// The icon is optional, a missing file gives empty data, which turns that default off
fn load_icon(path: &Path) -> egui::IconData {
    fs::read(path)
        .ok()
        .and_then(|png| eframe::icon_data::from_png_bytes(&png).ok())
        .unwrap_or_default()
}

fn app_icon() -> egui::IconData {
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.parent()?.join("Resources/KoDrop-icon.png")));
    bundled.map_or_else(egui::IconData::default, |path| load_icon(&path))
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 720.0])
            .with_min_inner_size([760.0, 720.0])
            .with_icon(app_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "KoDrop",
        options,
        Box::new(|cc| Ok(Box::new(KoDrop::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_icon_is_empty() {
        let icon = load_icon(Path::new("/nonexistent/KoDrop-icon.png"));
        assert_eq!((icon.width, icon.height), (0, 0));
    }
}
