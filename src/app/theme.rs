use eframe::egui;
use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId};
use std::sync::Arc;

pub(super) const GROUND: Color32 = Color32::from_rgb(0xEC, 0xED, 0xE8);
pub(super) const PANEL: Color32 = Color32::from_rgb(0xDF, 0xE1, 0xDB);
pub(super) const LINE: Color32 = Color32::from_rgb(0xCD, 0xD0, 0xC8);
pub(super) const INK: Color32 = Color32::from_rgb(0x16, 0x18, 0x1A);
pub(super) const MUTED: Color32 = Color32::from_rgb(0x4A, 0x50, 0x58);
pub(super) const FIELD: Color32 = Color32::from_rgb(0xF4, 0xF5, 0xF1);
pub(super) const FIELD_LINE: Color32 = Color32::from_rgb(0xB4, 0xB7, 0xAE);
pub(super) const TRACK: Color32 = Color32::from_rgb(0xC3, 0xC6, 0xBD);
pub(super) const DASH: Color32 = Color32::from_rgb(0xA4, 0xA7, 0x9E);
pub(super) const ACCENT: Color32 = Color32::from_rgb(0x1F, 0x47, 0xC4);
pub(super) const ACCENT_DARK: Color32 = Color32::from_rgb(0x16, 0x34, 0x8F);
pub(super) const DROP_GROUND: Color32 = Color32::from_rgb(0xE3, 0xE7, 0xF3);
pub(super) const DROP_PRESSED: Color32 = Color32::from_rgb(0xD3, 0xDA, 0xEE);
pub(super) const GOOD: Color32 = Color32::from_rgb(0x16, 0x62, 0x3F);
pub(super) const BAD: Color32 = Color32::from_rgb(0xA8, 0x2A, 0x1E);
pub(super) const BEZEL: Color32 = Color32::from_rgb(0x1B, 0x1D, 0x1F);
pub(super) const EINK: Color32 = Color32::from_rgb(0xD9, 0xDA, 0xD2);
pub(super) const EINK_LINE: Color32 = Color32::from_rgb(0xA9, 0xAB, 0xA3);

pub(super) fn sans(size: f32) -> FontId {
    FontId::proportional(size)
}

pub(super) fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

pub(super) fn serif(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("serif".into()))
}

pub(super) fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

pub(super) fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts
            .font_data
            .insert(name.to_string(), Arc::new(FontData::from_static(bytes)));
    };
    add("plex-sans", include_bytes!("../../assets/fonts/IBMPlexSans-Regular.ttf"));
    add("plex-bold", include_bytes!("../../assets/fonts/IBMPlexSans-SemiBold.ttf"));
    add("plex-mono", include_bytes!("../../assets/fonts/IBMPlexMono-Regular.ttf"));
    add("plex-serif", include_bytes!("../../assets/fonts/IBMPlexSerif-SemiBold.ttf"));

    let fallback = fonts.families[&FontFamily::Proportional].clone();
    let with_fallback = |name: &str| {
        let mut list = vec![name.to_string()];
        list.extend(fallback.iter().cloned());
        list
    };
    let families = [
        (FontFamily::Proportional, with_fallback("plex-sans")),
        (FontFamily::Monospace, with_fallback("plex-mono")),
        (FontFamily::Name("bold".into()), with_fallback("plex-bold")),
        (FontFamily::Name("serif".into()), with_fallback("plex-serif")),
    ];
    fonts.families.extend(families);
    ctx.set_fonts(fonts);
}
