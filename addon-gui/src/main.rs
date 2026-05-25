#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod commands;
mod config_ops;
mod tray;

use tauri::Manager;

fn main() {
    let directive: tracing::level_filters::LevelFilter =
        "info".parse().unwrap_or(tracing::level_filters::LevelFilter::INFO);
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(directive),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            tray::create_tray(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_daemon_status,
            commands::reload_config,
            commands::test_shortcut,
            commands::list_keybindings,
            commands::add_keybinding,
            commands::remove_keybinding,
            commands::export_config,
        ])
        .run(tauri::generate_context!())
        .inspect_err(|e| eprintln!("addon GUI crashed: {}", e));
}
