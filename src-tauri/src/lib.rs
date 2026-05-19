use rdev::{listen, Event, EventType};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Mutex,
    thread,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{Menu, MenuItem, Submenu},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use zip::ZipArchive;

const PET_WINDOW_LABEL: &str = "pet-overlay";
const DEFAULT_POSITION: &str = "bottom-right";
const DEFAULT_SIZE: &str = "medium";
const DEFAULT_OPACITY: f64 = 1.0;
const STORE_FILE_NAME: &str = "desktop-pet-state.json";
const DEMO_PET_ID: &str = "demo";
const MAX_PETPACK_ENTRIES: usize = 128;
const MAX_PETPACK_FILE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_PETPACK_TOTAL_BYTES: u64 = 50 * 1024 * 1024;
const PETPACK_METADATA_FILE: &str = "petpack.json";
const PETPACK_LICENSE_FILE: &str = "license.json";
const PETPACK_SIGNATURE_FILE: &str = "signature.ed25519";
const PETPACK_MANIFEST_FILE: &str = "manifest.json";
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
struct PetSkinCatalogItem {
    id: String,
    name: String,
    price: u64,
    description: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct PetManifest {
    id: String,
    name: String,
    status: String,
    description: String,
    #[serde(rename = "previewFrame")]
    preview_frame: String,
    #[serde(rename = "idleFrame")]
    idle_frame: String,
    #[serde(rename = "activeFrames")]
    active_frames: Vec<String>,
    #[serde(rename = "supportsSkins")]
    supports_skins: bool,
    skins: Vec<PetSkinCatalogItem>,
    source: Option<String>,
}

#[derive(Clone, Deserialize)]
struct PetpackAssetDigest {
    path: String,
    sha256: String,
}

#[derive(Clone, Deserialize)]
struct PetpackMetadata {
    #[serde(rename = "schemaVersion")]
    schema_version: u64,
    #[serde(rename = "packageId")]
    package_id: String,
    #[serde(rename = "petId")]
    pet_id: String,
    #[serde(rename = "petVersion")]
    pet_version: String,
    #[serde(rename = "minimumAppVersion")]
    minimum_app_version: String,
    #[serde(rename = "manifestPath")]
    manifest_path: String,
    #[serde(rename = "licensePath")]
    license_path: String,
    assets: Vec<PetpackAssetDigest>,
}

#[derive(Clone, Deserialize)]
struct LicenseSubject {
    #[serde(rename = "type")]
    subject_type: String,
    id: String,
}

#[derive(Clone, Deserialize)]
struct PetpackLicense {
    #[serde(rename = "licenseId")]
    license_id: String,
    #[serde(rename = "entitlementId")]
    entitlement_id: String,
    subject: LicenseSubject,
    #[serde(rename = "petId")]
    pet_id: String,
    #[serde(rename = "petVersion")]
    pet_version: String,
    #[serde(rename = "issuedAt")]
    issued_at: String,
    #[serde(rename = "expiresAt")]
    expires_at: Option<String>,
    #[serde(rename = "revalidateAfter")]
    revalidate_after: Option<String>,
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

fn pets_install_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let pets_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("pets");
    fs::create_dir_all(&pets_dir).map_err(|error| error.to_string())?;
    Ok(pets_dir)
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;

    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let entry_path = entry.path();
        let destination_path = destination.join(entry.file_name());

        if entry_path.is_dir() {
            copy_dir_recursive(&entry_path, &destination_path)?;
        } else {
            fs::copy(&entry_path, &destination_path).map_err(|error| error.to_string())?;
        }
    }

    Ok(())
}

fn temporary_import_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    let temp_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("imports-temp")
        .join(format!("pet-import-{millis}"));
    fs::create_dir_all(&temp_dir).map_err(|error| error.to_string())?;
    Ok(temp_dir)
}

fn extract_zip_to_dir(zip_path: &Path, destination: &Path) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
    if archive.len() > MAX_PETPACK_ENTRIES {
        return Err("petpack contains too many files".to_string());
    }

    let mut total_extracted_bytes = 0_u64;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let Some(relative_path) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            return Err("petpack contains an unsafe path".to_string());
        };

        let output_path = destination.join(relative_path);
        if entry.is_dir() {
            fs::create_dir_all(&output_path).map_err(|error| error.to_string())?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        let mut output_file = fs::File::create(&output_path).map_err(|error| error.to_string())?;
        let remaining_total_bytes = MAX_PETPACK_TOTAL_BYTES - total_extracted_bytes;
        let entry_limit = MAX_PETPACK_FILE_BYTES.min(remaining_total_bytes);
        let bytes_copied = io::copy(&mut entry.by_ref().take(entry_limit + 1), &mut output_file)
            .map_err(|error| error.to_string())?;

        if bytes_copied > MAX_PETPACK_FILE_BYTES {
            let _ = fs::remove_file(&output_path);
            return Err("petpack contains a file that is too large".to_string());
        }

        if total_extracted_bytes + bytes_copied > MAX_PETPACK_TOTAL_BYTES {
            let _ = fs::remove_file(&output_path);
            return Err("petpack is too large".to_string());
        }

        total_extracted_bytes += bytes_copied;
    }

    Ok(())
}

fn collect_manifest_dirs(root: &Path, found: &mut Vec<PathBuf>) -> Result<(), String> {
    let manifest_path = root.join("manifest.json");
    if manifest_path.exists() {
        found.push(root.to_path_buf());
    }

    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_manifest_dirs(&path, found)?;
        }
    }

    Ok(())
}

fn find_manifest_dir(root: &Path) -> Result<PathBuf, String> {
    let mut found = Vec::new();
    collect_manifest_dirs(root, &mut found)?;

    match found.len() {
        0 => Err("manifest.json not found in extracted petpack".to_string()),
        1 => Ok(found.remove(0)),
        _ => Err("petpack must contain exactly one manifest.json".to_string()),
    }
}

fn is_valid_pet_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
}

fn asset_path_within(manifest_dir: &Path, asset_path: &str) -> Result<PathBuf, String> {
    let candidate = manifest_dir.join(asset_path);
    let canonical_candidate = candidate
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let canonical_root = manifest_dir
        .canonicalize()
        .map_err(|error| error.to_string())?;

    if !canonical_candidate.starts_with(&canonical_root) {
        return Err("asset path escapes the pet folder".to_string());
    }

    Ok(canonical_candidate)
}

fn read_pet_manifest_from_path(manifest_path: &Path) -> Result<PetManifest, String> {
    let content = fs::read_to_string(manifest_path).map_err(|error| error.to_string())?;
    let manifest =
        serde_json::from_str::<PetManifest>(&content).map_err(|error| error.to_string())?;

    if manifest.id.trim().is_empty() {
        return Err("manifest pet id is required".to_string());
    }

    if !is_valid_pet_id(&manifest.id) {
        return Err("manifest pet id must be a safe slug".to_string());
    }

    if manifest.name.trim().is_empty() {
        return Err("manifest pet name is required".to_string());
    }

    Ok(manifest)
}

fn resolve_manifest_asset(manifest_dir: &Path, asset_path: &str) -> Result<String, String> {
    let path = Path::new(asset_path);
    if path.is_absolute() {
        return Ok(asset_path.to_string());
    }

    Ok(asset_path_within(manifest_dir, asset_path)?
        .to_string_lossy()
        .to_string())
}

fn read_json_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let content = fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str::<T>(&content).map_err(|error| error.to_string())
}

fn collect_named_file_dirs(
    root: &Path,
    file_name: &str,
    found: &mut Vec<PathBuf>,
) -> Result<(), String> {
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_named_file_dirs(&path, file_name, found)?;
        } else if entry.file_name().to_string_lossy() == file_name {
            let parent = path
                .parent()
                .ok_or_else(|| "petpack metadata path has no parent".to_string())?;
            found.push(parent.to_path_buf());
        }
    }

    Ok(())
}

fn single_metadata_dir(package_root: &Path, file_name: &str) -> Result<Option<PathBuf>, String> {
    let mut found = Vec::new();
    collect_named_file_dirs(package_root, file_name, &mut found)?;

    match found.len() {
        0 => Ok(None),
        1 => Ok(Some(found.remove(0))),
        _ => Err(format!("petpack must contain exactly one {file_name}")),
    }
}

fn validate_petpack_v2_metadata(
    package_root: &Path,
    manifest_dir: &Path,
    manifest: &PetManifest,
) -> Result<(), String> {
    let petpack_dir = single_metadata_dir(package_root, PETPACK_METADATA_FILE)?;
    let license_dir = single_metadata_dir(package_root, PETPACK_LICENSE_FILE)?;
    let signature_dir = single_metadata_dir(package_root, PETPACK_SIGNATURE_FILE)?;

    let has_any_v2_metadata =
        petpack_dir.is_some() || license_dir.is_some() || signature_dir.is_some();
    if !has_any_v2_metadata {
        return Ok(());
    }

    let Some(petpack_dir) = petpack_dir else {
        return Err("commercial petpack metadata is incomplete".to_string());
    };
    let Some(license_dir) = license_dir else {
        return Err("commercial petpack metadata is incomplete".to_string());
    };
    let Some(signature_dir) = signature_dir else {
        return Err("commercial petpack metadata is incomplete".to_string());
    };

    if petpack_dir != manifest_dir || license_dir != manifest_dir || signature_dir != manifest_dir {
        return Err("commercial petpack metadata must be next to manifest.json".to_string());
    }

    let petpack_path = manifest_dir.join(PETPACK_METADATA_FILE);
    let license_path = manifest_dir.join(PETPACK_LICENSE_FILE);
    let signature_path = manifest_dir.join(PETPACK_SIGNATURE_FILE);

    if !petpack_path.is_file() || !license_path.is_file() || !signature_path.is_file() {
        return Err("commercial petpack metadata is incomplete".to_string());
    }

    let petpack = read_json_file::<PetpackMetadata>(&petpack_path)?;
    let license = read_json_file::<PetpackLicense>(&license_path)?;

    if petpack.schema_version != 2 {
        return Err("unsupported petpack schema version".to_string());
    }

    if petpack.package_id.trim().is_empty()
        || petpack.pet_version.trim().is_empty()
        || petpack.minimum_app_version.trim().is_empty()
    {
        return Err("petpack metadata has required empty fields".to_string());
    }

    if petpack.pet_id != manifest.id || license.pet_id != manifest.id {
        return Err("petpack pet id does not match manifest".to_string());
    }

    if license.pet_version != petpack.pet_version {
        return Err("petpack license version does not match package version".to_string());
    }

    if license.license_id.trim().is_empty()
        || license.entitlement_id.trim().is_empty()
        || license.issued_at.trim().is_empty()
        || license.subject.id.trim().is_empty()
        || license.subject.subject_type != "account"
    {
        return Err("petpack license has invalid account metadata".to_string());
    }

    if license.expires_at.is_some() || license.revalidate_after.is_some() {
        return Err("expiring or revalidating petpack licenses are not supported yet".to_string());
    }

    if petpack.manifest_path != PETPACK_MANIFEST_FILE
        || petpack.license_path != PETPACK_LICENSE_FILE
    {
        return Err("petpack metadata paths are invalid".to_string());
    }

    if petpack.assets.is_empty() {
        return Err("petpack must declare asset hashes".to_string());
    }

    let mut declared_assets = HashSet::new();
    for asset in &petpack.assets {
        if asset.path.trim().is_empty() || asset.sha256.trim().is_empty() {
            return Err("petpack asset metadata is invalid".to_string());
        }
        asset_path_within(manifest_dir, &asset.path)?;
        declared_assets.insert(asset.path.clone());
    }

    let mut runtime_assets = vec![manifest.preview_frame.clone(), manifest.idle_frame.clone()];
    runtime_assets.extend(manifest.active_frames.iter().cloned());
    for runtime_asset in runtime_assets {
        if !declared_assets.contains(&runtime_asset) {
            return Err("petpack metadata does not declare all runtime assets".to_string());
        }
    }

    Ok(())
}

fn validate_manifest_assets(manifest_dir: &Path, manifest: &PetManifest) -> Result<(), String> {
    asset_path_within(manifest_dir, &manifest.preview_frame)?;
    asset_path_within(manifest_dir, &manifest.idle_frame)?;

    for frame in &manifest.active_frames {
        asset_path_within(manifest_dir, frame)?;
    }

    Ok(())
}

fn list_installed_pet_manifests(app_handle: &AppHandle) -> Result<Vec<PetManifest>, String> {
    let pets_dir = pets_install_dir(app_handle)?;
    let mut manifests = Vec::new();

    for entry in fs::read_dir(&pets_dir).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }

        let manifest_path = entry_path.join("manifest.json");
        if !manifest_path.exists() {
            continue;
        }

        let manifest_dir = match manifest_path.parent() {
            Some(path) => path,
            None => continue,
        };

        let mut manifest = match read_pet_manifest_from_path(&manifest_path) {
            Ok(manifest) => manifest,
            Err(_) => continue,
        };

        if validate_manifest_assets(manifest_dir, &manifest).is_err() {
            continue;
        }

        manifest.preview_frame = resolve_manifest_asset(manifest_dir, &manifest.preview_frame)?;
        manifest.idle_frame = resolve_manifest_asset(manifest_dir, &manifest.idle_frame)?;
        manifest.active_frames = manifest
            .active_frames
            .iter()
            .map(|frame| resolve_manifest_asset(manifest_dir, frame))
            .collect::<Result<Vec<_>, _>>()?;
        manifest.source = Some("imported".to_string());
        manifests.push(manifest);
    }

    Ok(manifests)
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

fn emit_pet_catalog_changed(app: &AppHandle) {
    let _ = app.emit("pet-catalog-changed", true);
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
fn get_installed_pet_catalog(app: AppHandle) -> Result<Vec<PetManifest>, String> {
    list_installed_pet_manifests(&app)
}

fn pet_id_collision_error(pet_id: &str) -> String {
    format!("PET_ID_COLLISION:{pet_id}")
}

fn dev_imports_enabled() -> bool {
    cfg!(debug_assertions) || option_env!("DESKTOP_PET_DEV_IMPORTS") == Some("true")
}

fn ensure_dev_imports_enabled() -> Result<(), String> {
    if dev_imports_enabled() {
        Ok(())
    } else {
        Err("dev imports are disabled in this build".to_string())
    }
}

fn install_pet_from_directory(
    app: &AppHandle,
    app_state: &AppState,
    package_root: &Path,
    source_dir: &Path,
    overwrite_existing: bool,
) -> Result<PersistedState, String> {
    if !source_dir.is_dir() {
        return Err("selected path is not a directory".to_string());
    }

    let manifest_path = source_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err("manifest.json not found in selected folder".to_string());
    }

    let manifest = read_pet_manifest_from_path(&manifest_path)?;
    if manifest.id == DEMO_PET_ID {
        return Err("demo pet id is reserved".to_string());
    }

    validate_manifest_assets(source_dir, &manifest)?;
    validate_petpack_v2_metadata(package_root, source_dir, &manifest)?;

    let pets_root = pets_install_dir(app)?;
    let install_dir = pets_root.join(&manifest.id);
    if !install_dir.starts_with(&pets_root) {
        return Err("pet id resolves outside the install directory".to_string());
    }

    let replacing_existing = install_dir.exists();
    if replacing_existing && !overwrite_existing {
        return Err(pet_id_collision_error(&manifest.id));
    }

    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    let staging_dir = pets_root.join(format!(".{}-staging-{millis}", manifest.id));
    let backup_dir = pets_root.join(format!(".{}-backup-{millis}", manifest.id));

    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir).map_err(|error| error.to_string())?;
    }
    copy_dir_recursive(source_dir, &staging_dir)?;

    if replacing_existing {
        fs::rename(&install_dir, &backup_dir).map_err(|error| error.to_string())?;
        if let Err(error) = fs::rename(&staging_dir, &install_dir) {
            let _ = fs::rename(&backup_dir, &install_dir);
            let _ = fs::remove_dir_all(&staging_dir);
            return Err(error.to_string());
        }
        let _ = fs::remove_dir_all(&backup_dir);
    } else if let Err(error) = fs::rename(&staging_dir, &install_dir) {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(error.to_string());
    }

    let snapshot = {
        let mut persisted = app_state.inner.lock().map_err(|error| error.to_string())?;
        ensure_demo_pet(&mut persisted);
        if !persisted
            .pets
            .downloaded_pets
            .iter()
            .any(|downloaded_pet| downloaded_pet == &manifest.id)
        {
            persisted.pets.downloaded_pets.push(manifest.id.clone());
        }
        persisted.pets.active_pet_id = manifest.id.clone();
        let snapshot = persisted.clone();
        save_persisted_state(app_state, &snapshot)?;
        snapshot
    };

    emit_pet_library_state(app, &snapshot.pets);
    emit_active_pet(app, &snapshot.pets.active_pet_id);
    emit_pet_catalog_changed(app);
    Ok(snapshot)
}

#[tauri::command]
fn import_pet_from_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    folder_path: String,
    overwrite_existing: bool,
) -> Result<PersistedState, String> {
    ensure_dev_imports_enabled()?;
    let source_dir = PathBuf::from(folder_path);
    install_pet_from_directory(
        &app,
        state.inner(),
        &source_dir,
        &source_dir,
        overwrite_existing,
    )
}

#[tauri::command]
fn import_petpack_file(
    app: AppHandle,
    state: State<'_, AppState>,
    file_path: String,
    overwrite_existing: bool,
) -> Result<PersistedState, String> {
    ensure_dev_imports_enabled()?;
    let source_file = PathBuf::from(file_path);
    if !source_file.is_file() {
        return Err("selected path is not a file".to_string());
    }

    let extraction_dir = temporary_import_dir(&app)?;
    let result = (|| {
        extract_zip_to_dir(&source_file, &extraction_dir)?;
        let pet_dir = find_manifest_dir(&extraction_dir)?;
        install_pet_from_directory(
            &app,
            state.inner(),
            &extraction_dir,
            &pet_dir,
            overwrite_existing,
        )
    })();
    let _ = fs::remove_dir_all(&extraction_dir);
    result
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
        .plugin(tauri_plugin_dialog::init())
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
            get_installed_pet_catalog,
            set_activity_tracking_enabled,
            import_pet_from_folder,
            import_petpack_file,
            set_active_pet,
            unlock_skin,
            set_active_skin
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
