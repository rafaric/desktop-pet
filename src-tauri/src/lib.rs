use rdev::{listen, Event, EventType};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::PathBuf, sync::Mutex, thread};
use tauri::{
    menu::{Menu, MenuItem, Submenu},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

const PET_WINDOW_LABEL: &str = "pet-overlay";
const DEFAULT_POSITION: &str = "bottom-right";
const DEFAULT_SIZE: &str = "medium";
const DEFAULT_OPACITY: f64 = 1.0;
const STORE_FILE_NAME: &str = "desktop-pet-state.json";
const DEMO_PET_ID: &str = "demo";
const DEFAULT_SKIN_ID: &str = "default";
const DEMO_SKINS: &[(&str, u64)] = &[("default", 0), ("mint", 25), ("berry", 50), ("night", 100)];

#[derive(Clone, Serialize, Deserialize)]
struct ActivitySnapshot {
    points: u64,
    mouse_clicks: u64,
    key_presses: u64,
    tracking_enabled: bool,
    pet_active: bool,
}

impl Default for ActivitySnapshot {
    fn default() -> Self {
        Self {
            points: 0,
            mouse_clicks: 0,
            key_presses: 0,
            tracking_enabled: true,
            pet_active: false,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct PetSettings {
    position: String,
    size: String,
    opacity: f64,
}

impl Default for PetSettings {
    fn default() -> Self {
        Self {
            position: DEFAULT_POSITION.to_string(),
            size: DEFAULT_SIZE.to_string(),
            opacity: DEFAULT_OPACITY,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct SkinState {
    active_skin_id: String,
    unlocked_skins: HashMap<String, Vec<String>>,
}

impl Default for SkinState {
    fn default() -> Self {
        let mut unlocked_skins = HashMap::new();
        unlocked_skins.insert(DEMO_PET_ID.to_string(), vec![DEFAULT_SKIN_ID.to_string()]);

        Self {
            active_skin_id: DEFAULT_SKIN_ID.to_string(),
            unlocked_skins,
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct PetLibraryState {
    active_pet_id: String,
    downloaded_pets: Vec<String>,
}

impl Default for PetLibraryState {
    fn default() -> Self {
        Self {
            active_pet_id: DEMO_PET_ID.to_string(),
            downloaded_pets: vec![DEMO_PET_ID.to_string()],
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct PersistedState {
    #[serde(default)]
    activity: ActivitySnapshot,
    #[serde(default)]
    settings: PetSettings,
    #[serde(default)]
    skins: SkinState,
    #[serde(default)]
    pets: PetLibraryState,
}

#[derive(Clone, Serialize)]
struct ActivityEventPayload {
    activity_kind: &'static str,
    stats: ActivitySnapshot,
}

struct AppState {
    inner: Mutex<PersistedState>,
    store_path: PathBuf,
}

fn pet_size(size: &str) -> u32 {
    match size {
        "small" => 160,
        "large" => 320,
        _ => 220,
    }
}

fn app_state_path(app: &tauri::App) -> Result<PathBuf, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&app_data_dir).map_err(|error| error.to_string())?;
    Ok(app_data_dir.join(STORE_FILE_NAME))
}

fn ensure_demo_pet(state: &mut PersistedState) {
    if !state
        .pets
        .downloaded_pets
        .iter()
        .any(|pet| pet == DEMO_PET_ID)
    {
        state.pets.downloaded_pets.push(DEMO_PET_ID.to_string());
    }

    if state.pets.active_pet_id.is_empty()
        || !state
            .pets
            .downloaded_pets
            .iter()
            .any(|pet| pet == &state.pets.active_pet_id)
    {
        state.pets.active_pet_id = DEMO_PET_ID.to_string();
    }
}

fn ensure_default_skin(state: &mut PersistedState) {
    let unlocked = state
        .skins
        .unlocked_skins
        .entry(DEMO_PET_ID.to_string())
        .or_default();

    if !unlocked.iter().any(|skin| skin == DEFAULT_SKIN_ID) {
        unlocked.push(DEFAULT_SKIN_ID.to_string());
    }

    if state.skins.active_skin_id.is_empty()
        || !unlocked
            .iter()
            .any(|skin| skin == &state.skins.active_skin_id)
    {
        state.skins.active_skin_id = DEFAULT_SKIN_ID.to_string();
    }
}

fn load_persisted_state(path: &PathBuf) -> PersistedState {
    let mut state = fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<PersistedState>(&content).ok())
        .unwrap_or_default();
    ensure_demo_pet(&mut state);
    ensure_default_skin(&mut state);
    state
}

fn save_persisted_state(state: &AppState, snapshot: &PersistedState) -> Result<(), String> {
    let content = serde_json::to_string_pretty(snapshot).map_err(|error| error.to_string())?;
    fs::write(&state.store_path, content).map_err(|error| error.to_string())
}

fn get_pet_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window(PET_WINDOW_LABEL)
        .ok_or_else(|| "pet overlay window is not available".to_string())
}

fn emit_activity_stats(app: &AppHandle, stats: &ActivitySnapshot) {
    let _ = app.emit("activity-stats-updated", stats);
}

fn emit_settings(app: &AppHandle, settings: &PetSettings) {
    let _ = app.emit("pet-settings-updated", settings);
}

fn emit_skin_state(app: &AppHandle, skins: &SkinState) {
    let _ = app.emit("skin-state-updated", skins);
}

fn emit_pet_library_state(app: &AppHandle, pets: &PetLibraryState) {
    let _ = app.emit("pet-library-updated", pets);
}

fn emit_active_pet(app: &AppHandle, pet_id: &str) {
    let _ = app.emit("pet-active-changed", pet_id);
}

fn update_persisted_state<F>(app_state: &AppState, update: F) -> Result<PersistedState, String>
where
    F: FnOnce(&mut PersistedState),
{
    let mut state = app_state.inner.lock().map_err(|error| error.to_string())?;
    update(&mut state);
    let snapshot = state.clone();
    save_persisted_state(app_state, &snapshot)?;
    Ok(snapshot)
}

fn record_activity(
    app: &AppHandle,
    app_state: &AppState,
    activity_kind: &'static str,
) -> Result<(), String> {
    let payload = {
        let mut state = app_state.inner.lock().map_err(|error| error.to_string())?;

        if !state.activity.tracking_enabled || !state.activity.pet_active {
            return Ok(());
        }

        state.activity.points += 1;
        match activity_kind {
            "mouse" => state.activity.mouse_clicks += 1,
            "keyboard" => state.activity.key_presses += 1,
            _ => {}
        }

        let snapshot = state.clone();
        save_persisted_state(app_state, &snapshot)?;

        ActivityEventPayload {
            activity_kind,
            stats: snapshot.activity,
        }
    };

    let _ = app.emit("activity-detected", &payload);
    let _ = app.emit("activity-stats-updated", &payload.stats);
    Ok(())
}

fn start_activity_listener(app: AppHandle) {
    let listener_app = app.clone();

    thread::spawn(move || {
        let callback = move |event: Event| match event.event_type {
            EventType::ButtonPress(_) => {
                let state = listener_app.state::<AppState>();
                let _ = record_activity(&listener_app, state.inner(), "mouse");
            }
            EventType::KeyPress(_) => {
                let state = listener_app.state::<AppState>();
                let _ = record_activity(&listener_app, state.inner(), "keyboard");
            }
            _ => {}
        };

        if let Err(error) = listen(callback) {
            let _ = app.emit("activity-listener-error", format!("{error:?}"));
        }
    });
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

fn apply_pet_settings(app: &AppHandle, settings: &PetSettings) -> Result<(), String> {
    resize_pet(app, &settings.size)?;
    move_pet_to_position(app, &settings.position, pet_size(&settings.size))?;
    set_pet_opacity_event(app, settings.opacity)
}

fn set_pet_opacity_event(app: &AppHandle, opacity: f64) -> Result<(), String> {
    get_pet_window(app)?
        .emit("pet-opacity-changed", opacity)
        .map_err(|error| error.to_string())
}

fn set_pet_skin_event(app: &AppHandle, skin_id: &str) -> Result<(), String> {
    get_pet_window(app)?
        .emit("pet-skin-changed", skin_id)
        .map_err(|error| error.to_string())
}

fn skin_price(pet_id: &str, skin_id: &str) -> Option<u64> {
    if pet_id != DEMO_PET_ID {
        return None;
    }

    DEMO_SKINS
        .iter()
        .find_map(|(id, price)| (*id == skin_id).then_some(*price))
}

fn is_skin_unlocked(skins: &SkinState, pet_id: &str, skin_id: &str) -> bool {
    skins
        .unlocked_skins
        .get(pet_id)
        .is_some_and(|unlocked| unlocked.iter().any(|skin| skin == skin_id))
}

#[tauri::command]
fn show_pet(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let window = get_pet_window(&app)?;
    let snapshot = update_persisted_state(state.inner(), |persisted| {
        persisted.activity.pet_active = true;
    })?;

    apply_pet_settings(&app, &snapshot.settings)?;
    window.show().map_err(|error| error.to_string())?;
    window
        .set_always_on_top(true)
        .map_err(|error| error.to_string())?;

    emit_activity_stats(&app, &snapshot.activity);
    emit_settings(&app, &snapshot.settings);
    emit_skin_state(&app, &snapshot.skins);
    emit_pet_library_state(&app, &snapshot.pets);
    emit_active_pet(&app, &snapshot.pets.active_pet_id);
    set_pet_skin_event(&app, &snapshot.skins.active_skin_id)?;
    Ok(())
}

#[tauri::command]
fn hide_pet(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    get_pet_window(&app)?
        .hide()
        .map_err(|error| error.to_string())?;

    let snapshot = update_persisted_state(state.inner(), |persisted| {
        persisted.activity.pet_active = false;
    })?;
    emit_activity_stats(&app, &snapshot.activity);
    Ok(())
}

#[tauri::command]
fn set_pet_position(
    app: AppHandle,
    state: State<'_, AppState>,
    position: String,
) -> Result<PetSettings, String> {
    let snapshot = update_persisted_state(state.inner(), |persisted| {
        persisted.settings.position = position;
    })?;
    move_pet_to_position(
        &app,
        &snapshot.settings.position,
        pet_size(&snapshot.settings.size),
    )?;
    emit_settings(&app, &snapshot.settings);
    Ok(snapshot.settings)
}

#[tauri::command]
fn set_pet_size(
    app: AppHandle,
    state: State<'_, AppState>,
    size: String,
) -> Result<PetSettings, String> {
    let snapshot = update_persisted_state(state.inner(), |persisted| {
        persisted.settings.size = size;
    })?;
    resize_pet(&app, &snapshot.settings.size)?;
    move_pet_to_position(
        &app,
        &snapshot.settings.position,
        pet_size(&snapshot.settings.size),
    )?;
    emit_settings(&app, &snapshot.settings);
    Ok(snapshot.settings)
}

#[tauri::command]
fn set_pet_opacity(
    app: AppHandle,
    state: State<'_, AppState>,
    opacity: f64,
) -> Result<PetSettings, String> {
    let snapshot = update_persisted_state(state.inner(), |persisted| {
        persisted.settings.opacity = opacity;
    })?;
    set_pet_opacity_event(&app, snapshot.settings.opacity)?;
    emit_settings(&app, &snapshot.settings);
    Ok(snapshot.settings)
}

#[tauri::command]
fn get_activity_stats(state: State<'_, AppState>) -> Result<ActivitySnapshot, String> {
    state
        .inner
        .lock()
        .map(|state| state.activity.clone())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_app_state(state: State<'_, AppState>) -> Result<PersistedState, String> {
    state
        .inner
        .lock()
        .map(|state| state.clone())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_activity_tracking_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<ActivitySnapshot, String> {
    let snapshot = update_persisted_state(state.inner(), |persisted| {
        persisted.activity.tracking_enabled = enabled;
    })?;

    emit_activity_stats(&app, &snapshot.activity);
    Ok(snapshot.activity)
}

#[tauri::command]
fn set_active_pet(
    app: AppHandle,
    state: State<'_, AppState>,
    pet_id: String,
) -> Result<PetLibraryState, String> {
    let snapshot = {
        let app_state = state.inner();
        let mut persisted = app_state.inner.lock().map_err(|error| error.to_string())?;
        ensure_demo_pet(&mut persisted);

        if !persisted
            .pets
            .downloaded_pets
            .iter()
            .any(|downloaded_pet| downloaded_pet == &pet_id)
        {
            return Err("pet is not downloaded".to_string());
        }

        persisted.pets.active_pet_id = pet_id;
        let snapshot = persisted.clone();
        save_persisted_state(app_state, &snapshot)?;
        snapshot
    };

    emit_pet_library_state(&app, &snapshot.pets);
    emit_active_pet(&app, &snapshot.pets.active_pet_id);
    Ok(snapshot.pets)
}

#[tauri::command]
fn unlock_skin(
    app: AppHandle,
    state: State<'_, AppState>,
    pet_id: String,
    skin_id: String,
) -> Result<PersistedState, String> {
    let price = skin_price(&pet_id, &skin_id).ok_or_else(|| "unknown skin".to_string())?;
    let snapshot = {
        let app_state = state.inner();
        let mut persisted = app_state.inner.lock().map_err(|error| error.to_string())?;
        ensure_default_skin(&mut persisted);

        if !is_skin_unlocked(&persisted.skins, &pet_id, &skin_id) {
            if persisted.activity.points < price {
                return Err("not enough points to unlock this skin".to_string());
            }

            persisted.activity.points -= price;
            persisted
                .skins
                .unlocked_skins
                .entry(pet_id)
                .or_default()
                .push(skin_id.clone());
        }

        persisted.skins.active_skin_id = skin_id;
        let snapshot = persisted.clone();
        save_persisted_state(app_state, &snapshot)?;
        snapshot
    };

    emit_activity_stats(&app, &snapshot.activity);
    emit_skin_state(&app, &snapshot.skins);
    set_pet_skin_event(&app, &snapshot.skins.active_skin_id)?;
    Ok(snapshot)
}

#[tauri::command]
fn set_active_skin(
    app: AppHandle,
    state: State<'_, AppState>,
    pet_id: String,
    skin_id: String,
) -> Result<SkinState, String> {
    let snapshot = {
        let app_state = state.inner();
        let mut persisted = app_state.inner.lock().map_err(|error| error.to_string())?;
        ensure_default_skin(&mut persisted);

        if !is_skin_unlocked(&persisted.skins, &pet_id, &skin_id) {
            return Err("skin is locked".to_string());
        }

        persisted.skins.active_skin_id = skin_id;
        let snapshot = persisted.clone();
        save_persisted_state(app_state, &snapshot)?;
        snapshot
    };

    emit_skin_state(&app, &snapshot.skins);
    set_pet_skin_event(&app, &snapshot.skins.active_skin_id)?;
    Ok(snapshot.skins)
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
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = show_pet(app.clone(), state);
                }
            }
            "hide_pet" => {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = hide_pet(app.clone(), state);
                }
            }
            "position_top_left" => {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = set_pet_position(app.clone(), state, "top-left".to_string());
                }
            }
            "position_top_right" => {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = set_pet_position(app.clone(), state, "top-right".to_string());
                }
            }
            "position_bottom_left" => {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = set_pet_position(app.clone(), state, "bottom-left".to_string());
                }
            }
            "position_bottom_right" => {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = set_pet_position(app.clone(), state, "bottom-right".to_string());
                }
            }
            "size_small" => {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = set_pet_size(app.clone(), state, "small".to_string());
                }
            }
            "size_medium" => {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = set_pet_size(app.clone(), state, "medium".to_string());
                }
            }
            "size_large" => {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = set_pet_size(app.clone(), state, "large".to_string());
                }
            }
            "opacity_100" => {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = set_pet_opacity(app.clone(), state, 1.0);
                }
            }
            "opacity_75" => {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = set_pet_opacity(app.clone(), state, 0.75);
                }
            }
            "opacity_50" => {
                if let Some(state) = app.try_state::<AppState>() {
                    let _ = set_pet_opacity(app.clone(), state, 0.5);
                }
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
            let store_path = app_state_path(app)
                .map_err(|error| tauri::Error::Io(std::io::Error::other(error)))?;
            let persisted = load_persisted_state(&store_path);
            app.manage(AppState {
                inner: Mutex::new(persisted.clone()),
                store_path,
            });

            create_pet_window(app)?;
            create_tray(app)?;

            if persisted.activity.pet_active {
                let handle = app.handle().clone();
                apply_pet_settings(&handle, &persisted.settings)?;
                get_pet_window(&handle)?.show()?;
            }

            start_activity_listener(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            show_pet,
            hide_pet,
            set_pet_position,
            set_pet_size,
            set_pet_opacity,
            get_activity_stats,
            get_app_state,
            set_activity_tracking_enabled,
            set_active_pet,
            unlock_skin,
            set_active_skin
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
