#![cfg_attr(windows, windows_subsystem = "windows")] // no console window on Windows

mod app;
mod clicker;
mod config;

use eframe::egui;

fn main() -> eframe::Result {
    // Window/taskbar icon (separate from the .exe icon embedded by build.rs).
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
        .expect("invalid icon.png");

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Rusty Clicker")
        .with_icon(icon)
        .with_inner_size([470.0, 350.0])
        .with_min_inner_size([470.0, 350.0])
        .with_resizable(false);
    // Restore last window position (saved by App::on_exit).
    if let Some(pos) = config::Config::load().window_pos {
        viewport = viewport.with_position(pos);
    }

    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "Rusty Clicker",
        options,
        Box::new(|cc| {
            load_system_fonts(&cc.egui_ctx);
            Ok(Box::new(app::App::new(cc)))
        }),
    )
}

/// Use the OS's own UI fonts instead of embedding ~1 MB of fonts in the binary.
/// Every font found is added as a fallback, so glyphs missing from the first
/// (e.g. "●") are taken from the next.
fn load_system_fonts(ctx: &egui::Context) {
    #[cfg(windows)]
    const CANDIDATES: &[&str] = &[
        "C:\\Windows\\Fonts\\segoeui.ttf",
        "C:\\Windows\\Fonts\\seguisym.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
    ];
    #[cfg(target_os = "macos")]
    const CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/Helvetica.ttc",
        "/System/Library/Fonts/Apple Symbols.ttf",
        "/Library/Fonts/Arial Unicode.ttf",
    ];
    #[cfg(all(unix, not(target_os = "macos")))]
    const CANDIDATES: &[&str] = &[
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/liberation-sans/LiberationSans-Regular.ttf",
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    ];

    let mut fonts = egui::FontDefinitions::empty();
    for (i, path) in CANDIDATES.iter().enumerate() {
        if let Ok(bytes) = std::fs::read(path) {
            let name = format!("sys{i}");
            fonts
                .font_data
                .insert(name.clone(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
            for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push(name.clone());
            }
        }
    }
    if !fonts.font_data.is_empty() {
        ctx.set_fonts(fonts);
    }
}
