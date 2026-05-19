use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::AppHandle;

pub fn create_tray(app: &AppHandle) -> Result<(), anyhow::Error> {
    let show = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let _tray = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("addon")
        .build(app)?;

    Ok(())
}
