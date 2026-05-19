//! # Tauri backend entry point
//!
//! Registers Tauri commands for the addon GUI and initializes the
//! application tray icon.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod config_ops;
mod ipc_client;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            // Initialize system tray icon if present.
            #[cfg(any(target_os = "macos", target_os = "linux"))]
            {
                use tauri::menu::{Menu, MenuItem};
                use tauri::tray::TrayIconBuilder;

                let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)
                    .expect("Error adding quit item");
                let menu = Menu::with_items(app, &[&quit])
                    .expect("Error creating menu");
                let _tray = TrayIconBuilder::with_id("main_tray")
                    .menu(&menu)
                    .tooltip("Addon")
                    .build(app)
                    .expect("Error building tray icon");
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_daemon_status,
            commands::reload_config,
            commands::test_shortcut,
            commands::list_installed_keybindings,
        ])
        .run(tauri::generate_context!())
        .expect("Error running Tauri application");
}
