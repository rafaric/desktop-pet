use tauri::{
    menu::{Menu, MenuItem, Submenu},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

const PET_WINDOW_LABEL: &str = "pet-overlay";
const DEFAULT_POSITION: &str = "bottom-right";
const DEFAULT_SIZE: &str = "medium";

fn pet_size(size: &str) -> u32 {
    match size {
        "small" => 160,
        "large" => 320,
        _ => 220,
    }
}

fn get_pet_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window(PET_WINDOW_LABEL)
        .ok_or_else(|| "pet overlay window is not available".to_string())
}

fn move_pet_to_position(app: &AppHandle, position: &str, size: u32) -> Result<(), String> {
    let window = get_pet_window(app)?;
    let monitor = window
        .current_monitor()
        .map_err(|error| error.to_string())?
        .or_else(|| app.primary_monitor().ok().flatten())
        .ok_or_else(|| "no monitor available".to_string())?;

    let monitor_position = monitor.position();
    let monitor_size = monitor.size();
    let margin = 24_i32;
    let width = size as i32;
    let height = size as i32;

    let x = match position {
        "top-left" | "bottom-left" => monitor_position.x + margin,
        "top-right" | "bottom-right" => {
            monitor_position.x + monitor_size.width as i32 - width - margin
        }
        _ => monitor_position.x + monitor_size.width as i32 - width - margin,
    };

    let y = match position {
        "top-left" | "top-right" => monitor_position.y + margin,
        "bottom-left" | "bottom-right" => {
            monitor_position.y + monitor_size.height as i32 - height - margin
        }
        _ => monitor_position.y + monitor_size.height as i32 - height - margin,
    };

    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|error| error.to_string())
}

fn resize_pet(app: &AppHandle, size: &str) -> Result<(), String> {
    let window = get_pet_window(app)?;
    let pixels = pet_size(size);

    window
        .set_size(PhysicalSize::new(pixels, pixels))
        .map_err(|error| error.to_string())?;
    window
        .emit("pet-size-changed", size)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn show_pet(app: AppHandle) -> Result<(), String> {
    let window = get_pet_window(&app)?;
    resize_pet(&app, DEFAULT_SIZE)?;
    move_pet_to_position(&app, DEFAULT_POSITION, pet_size(DEFAULT_SIZE))?;
    window.show().map_err(|error| error.to_string())?;
    window
        .set_always_on_top(true)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn hide_pet(app: AppHandle) -> Result<(), String> {
    get_pet_window(&app)?
        .hide()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_pet_position(app: AppHandle, position: String) -> Result<(), String> {
    let window = get_pet_window(&app)?;
    let size = window
        .inner_size()
        .map_err(|error| error.to_string())?
        .width;
    move_pet_to_position(&app, &position, size)
}

#[tauri::command]
fn set_pet_size(app: AppHandle, size: String) -> Result<(), String> {
    let pixels = pet_size(&size);
    resize_pet(&app, &size)?;
    move_pet_to_position(&app, DEFAULT_POSITION, pixels)
}

#[tauri::command]
fn set_pet_opacity(app: AppHandle, opacity: f64) -> Result<(), String> {
    get_pet_window(&app)?
        .emit("pet-opacity-changed", opacity)
        .map_err(|error| error.to_string())
}

fn create_pet_window(app: &tauri::App) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, PET_WINDOW_LABEL, WebviewUrl::App("index.html".into()))
        .title("Desktop Pet")
        .inner_size(pet_size(DEFAULT_SIZE) as f64, pet_size(DEFAULT_SIZE) as f64)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .focusable(false)
        .focused(false)
        .visible(false)
        .build()?;

    Ok(())
}

fn create_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show_pet", "Show pet", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide_pet", "Hide pet", true, None::<&str>)?;
    let open = MenuItem::with_id(app, "open_app", "Open app", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let top_left = MenuItem::with_id(app, "position_top_left", "Top left", true, None::<&str>)?;
    let top_right = MenuItem::with_id(app, "position_top_right", "Top right", true, None::<&str>)?;
    let bottom_left = MenuItem::with_id(
        app,
        "position_bottom_left",
        "Bottom left",
        true,
        None::<&str>,
    )?;
    let bottom_right = MenuItem::with_id(
        app,
        "position_bottom_right",
        "Bottom right",
        true,
        None::<&str>,
    )?;
    let position = Submenu::with_items(
        app,
        "Position",
        true,
        &[&top_left, &top_right, &bottom_left, &bottom_right],
    )?;

    let small = MenuItem::with_id(app, "size_small", "Small", true, None::<&str>)?;
    let medium = MenuItem::with_id(app, "size_medium", "Medium", true, None::<&str>)?;
    let large = MenuItem::with_id(app, "size_large", "Large", true, None::<&str>)?;
    let size = Submenu::with_items(app, "Size", true, &[&small, &medium, &large])?;

    let opacity_100 = MenuItem::with_id(app, "opacity_100", "100%", true, None::<&str>)?;
    let opacity_75 = MenuItem::with_id(app, "opacity_75", "75%", true, None::<&str>)?;
    let opacity_50 = MenuItem::with_id(app, "opacity_50", "50%", true, None::<&str>)?;
    let opacity = Submenu::with_items(
        app,
        "Opacity",
        true,
        &[&opacity_100, &opacity_75, &opacity_50],
    )?;

    let menu = Menu::with_items(
        app,
        &[&open, &show, &hide, &position, &size, &opacity, &quit],
    )?;

    TrayIconBuilder::new()
        .tooltip("Desktop Pet Companion")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open_app" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "show_pet" => {
                let _ = show_pet(app.clone());
            }
            "hide_pet" => {
                let _ = hide_pet(app.clone());
            }
            "position_top_left" => {
                let _ = set_pet_position(app.clone(), "top-left".to_string());
            }
            "position_top_right" => {
                let _ = set_pet_position(app.clone(), "top-right".to_string());
            }
            "position_bottom_left" => {
                let _ = set_pet_position(app.clone(), "bottom-left".to_string());
            }
            "position_bottom_right" => {
                let _ = set_pet_position(app.clone(), "bottom-right".to_string());
            }
            "size_small" => {
                let _ = set_pet_size(app.clone(), "small".to_string());
            }
            "size_medium" => {
                let _ = set_pet_size(app.clone(), "medium".to_string());
            }
            "size_large" => {
                let _ = set_pet_size(app.clone(), "large".to_string());
            }
            "opacity_100" => {
                let _ = set_pet_opacity(app.clone(), 1.0);
            }
            "opacity_75" => {
                let _ = set_pet_opacity(app.clone(), 0.75);
            }
            "opacity_50" => {
                let _ = set_pet_opacity(app.clone(), 0.5);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            create_pet_window(app)?;
            create_tray(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            show_pet,
            hide_pet,
            set_pet_position,
            set_pet_size,
            set_pet_opacity
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
