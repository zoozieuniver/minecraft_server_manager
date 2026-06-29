#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod whitelist;
mod server_manager;
mod mod_manager;
mod tailscale;
mod app;

use app::MinecraftManagerApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Minecraft Server Manager")
            .with_inner_size([720.0, 520.0]), // Трохи збільшимо розмір вікна для зручності двопанельного перегляду
        ..Default::default()
    };

    eframe::run_native(
        "mc_server_gui",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Box::new(MinecraftManagerApp::new(cc))
        }),
    )
}
