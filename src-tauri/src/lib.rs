mod mist;

use mist::{MistClient, OrgInfo, SiteInfo};
use serde_json::Value;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};
use tauri_plugin_positioner::{Position, WindowExt};
use tauri_plugin_store::StoreExt;

const STORE_FILE: &str = "config.json";
const CONFIG_KEY: &str = "config";

// ---------------- config persistence ----------------

#[tauri::command]
fn load_config(app: tauri::AppHandle) -> Result<Value, String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    Ok(store.get(CONFIG_KEY).unwrap_or(Value::Null))
}

#[tauri::command]
fn save_config(app: tauri::AppHandle, config: Value) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    store.set(CONFIG_KEY, config);
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------- Mist API commands ----------------

#[tauri::command]
async fn get_self(host: String, token: String) -> Result<Vec<OrgInfo>, String> {
    MistClient::new(&host, &token)?.get_self().await
}

#[tauri::command]
async fn get_sites(host: String, token: String, org_id: String) -> Result<Vec<SiteInfo>, String> {
    MistClient::new(&host, &token)?.get_sites(&org_id).await
}

#[tauri::command]
async fn get_dashboard(
    host: String,
    token: String,
    org_id: String,
    site_id: Option<String>,
) -> Result<Value, String> {
    let client = MistClient::new(&host, &token)?;
    Ok(client.get_dashboard(&org_id, site_id.as_deref()).await)
}

#[tauri::command]
fn dashboard_url(host: String, org_id: String, site_id: Option<String>) -> String {
    MistClient::dashboard_url(&host, &org_id, site_id.as_deref())
}

#[tauri::command]
fn set_tray_title(app: tauri::AppHandle, title: String) {
    if let Some(tray) = app.tray_by_id("main") {
        let t = if title.trim().is_empty() {
            None
        } else {
            Some(title)
        };
        let _ = tray.set_title(t);
    }
}

// ---------------- app entry ----------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_positioner::init())
        .invoke_handler(tauri::generate_handler![
            load_config,
            save_config,
            get_self,
            get_sites,
            get_dashboard,
            dashboard_url,
            set_tray_title,
        ])
        .on_window_event(|window, event| {
            // Hide the popover when it loses focus.
            if let tauri::WindowEvent::Focused(false) = event {
                let _ = window.hide();
            }
        })
        .setup(|app| {
            // Build tray icon (template image for dark/light auto-tinting).
            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;

            let quit = MenuItem::with_id(app, "quit", "Quit Mist", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&quit])?;

            TrayIconBuilder::with_id("main")
                .icon(icon)
                .icon_as_template(true)
                .tooltip("Mist Dashboard")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    if event.id.as_ref() == "quit" {
                        app.exit(0);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    let app = tray.app_handle();
                    tauri_plugin_positioner::on_tray_event(app, &event);
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Some(win) = app.get_webview_window("main") {
                            if win.is_visible().unwrap_or(false) {
                                let _ = win.hide();
                            } else {
                                let _ = win.move_window(Position::TrayCenter);
                                let _ = win.show();
                                let _ = win.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

            // macOS: keep app out of the Dock (activation policy Accessory).
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
