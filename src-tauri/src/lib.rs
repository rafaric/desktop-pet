use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use rdev::{listen, Event, EventType};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
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
const PETPACK_PUBLIC_KEY_BYTES: [u8; 32] = [
    0x44, 0x8b, 0xaf, 0xfa, 0xe2, 0x8e, 0x07, 0xa3, 0x45, 0x6b, 0xe7, 0x70, 0x2c, 0xc1, 0xe2, 0x03,
    0x3d, 0xdf, 0x9f, 0x8b, 0xdb, 0xc1, 0x7f, 0xf2, 0x2f, 0x71, 0xa8, 0xa6, 0xa8, 0xed, 0x1a, 0x89,
];
const DEVELOPMENT_ACCOUNT_ID: &str = "user_test";
const DEVELOPMENT_ACCOUNT_EMAIL: &str = "user_test@example.com";
const GOOGLE_CLIENT_ID: &str =
    "271056612612-ruesqtv1e3h4a1t3qf0e7fp3u0sim3gg.apps.googleusercontent.com";
const GOOGLE_CLIENT_SECRET: &str = "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";
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

#[derive(Clone, Deserialize, Serialize)]
struct PetpackAssetDigest {
    path: String,
    sha256: String,
}

#[derive(Clone, Deserialize, Serialize)]
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

#[derive(Clone, Deserialize, Serialize)]
struct LicenseSubject {
    #[serde(rename = "type")]
    subject_type: String,
    id: String,
}

#[derive(Clone, Deserialize, Serialize)]
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
#[serde(rename_all = "snake_case")]
struct AccountSession {
    id: String,
    email: String,
    display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    access_token: Option<String>,
}

impl Default for AccountSession {
    fn default() -> Self {
        Self {
            id: DEVELOPMENT_ACCOUNT_ID.to_string(),
            email: DEVELOPMENT_ACCOUNT_EMAIL.to_string(),
            display_name: "Development User".to_string(),
            access_token: None,
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct AccountState {
    #[serde(default)]
    account: AccountSession,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id_token: Option<String>,
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
    #[serde(default)]
    account: AccountState,
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

fn sha256_digest(path: &Path) -> Result<[u8; 32], String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher).map_err(|error| error.to_string())?;
    Ok(hasher.finalize().into())
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn digest_matches_declared(digest: &[u8], declared: &str) -> bool {
    let declared = declared.trim();
    encode_hex(digest).eq_ignore_ascii_case(declared)
        || BASE64_STANDARD
            .decode(declared)
            .is_ok_and(|decoded| decoded == digest)
}

fn canonical_signature_payload(
    petpack: &PetpackMetadata,
    license: &PetpackLicense,
) -> Result<Vec<u8>, String> {
    let payload = json!({
        "license": license,
        "petpack": petpack,
    });
    serde_json_canonicalizer::to_vec(&payload).map_err(|error| error.to_string())
}

fn verify_petpack_signature(
    canonical_payload: &[u8],
    signature_path: &Path,
    public_key_bytes: &[u8; 32],
) -> Result<(), String> {
    let signature_content =
        fs::read_to_string(signature_path).map_err(|error| error.to_string())?;
    let signature_bytes = BASE64_STANDARD
        .decode(signature_content.trim())
        .map_err(|_| "petpack signature is not valid base64".to_string())?;
    let signature = Signature::try_from(signature_bytes.as_slice())
        .map_err(|_| "petpack signature has invalid length".to_string())?;
    let verifying_key = VerifyingKey::from_bytes(public_key_bytes)
        .map_err(|_| "embedded petpack public key is invalid".to_string())?;

    verifying_key
        .verify(canonical_payload, &signature)
        .map_err(|_| "petpack signature verification failed".to_string())
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
    account_id: &str,
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
    let canonical_payload = canonical_signature_payload(&petpack, &license)?;
    verify_petpack_signature(
        &canonical_payload,
        &signature_path,
        &PETPACK_PUBLIC_KEY_BYTES,
    )?;

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

    if license.subject.id != account_id {
        return Err("petpack license belongs to another account".to_string());
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
        let asset_path = asset_path_within(manifest_dir, &asset.path)?;
        let digest = sha256_digest(&asset_path)?;
        if !digest_matches_declared(&digest, &asset.sha256) {
            return Err("petpack asset hash mismatch".to_string());
        }
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
fn get_current_account(state: State<'_, AppState>) -> Result<AccountSession, String> {
    let guard = state.inner.lock().map_err(|e| e.to_string())?;
    Ok(guard.account.account.clone())
}

#[tauri::command]
fn save_account_session(
    state: State<'_, AppState>,
    account: AccountSession,
    id_token: Option<String>,
) -> Result<AccountSession, String> {
    set_authenticated_account(state.inner(), account, id_token)
}

#[tauri::command]
fn clear_account_session(state: State<'_, AppState>) -> Result<(), String> {
    clear_account(state.inner())
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

// ─── Google Auth helpers ───────────────────────────────────────────────────────

fn current_account_id(state: &PersistedState) -> String {
    state.account.account.id.clone()
}

fn set_authenticated_account(
    app_state: &AppState,
    account: AccountSession,
    id_token: Option<String>,
) -> Result<AccountSession, String> {
    let mut guard = app_state.inner.lock().map_err(|e| e.to_string())?;
    guard.account.account = account.clone();
    guard.account.id_token = id_token;
    let snapshot = guard.clone();
    drop(guard);
    save_persisted_state(app_state, &snapshot).map_err(|e| e.to_string())?;
    Ok(account)
}

fn clear_account(app_state: &AppState) -> Result<(), String> {
    let mut guard = app_state.inner.lock().map_err(|e| e.to_string())?;
    guard.account = AccountState::default();
    let snapshot = guard.clone();
    drop(guard);
    save_persisted_state(app_state, &snapshot).map_err(|e| e.to_string())?;
    Ok(())
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

    let account_id = {
        let guard = app_state.inner.lock().map_err(|e| e.to_string())?;
        guard.account.account.id.clone()
    };
    validate_petpack_v2_metadata(package_root, source_dir, &manifest, &account_id)?;

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
        .tooltip("Desktop Pet")
        .icon(
            app.default_window_icon()
                .cloned()
                .unwrap_or_else(|| tauri::include_image!("icons/32x32.png").to_owned()),
        )
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, event| {
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                    } else {
                        // Position on the right edge of the primary monitor
                        if let Ok(Some(monitor)) = window.primary_monitor() {
                            let screen = monitor.size();
                            let scale = monitor.scale_factor();
                            let win_size =
                                window.outer_size().unwrap_or(PhysicalSize::new(380, 680));
                            let x = (screen.width as f64 / scale - win_size.width as f64 / scale)
                                as i32;
                            let y = 0_i32;
                            let _ = window
                                .set_position(PhysicalPosition::new((x as f64 * scale) as i32, y));
                        }
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
        })
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

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    const PETPACK_TEST_SECRET_KEY_BYTES: [u8; 32] = [
        0x8c, 0x67, 0x5d, 0x65, 0x93, 0xea, 0xa2, 0xe6, 0x4a, 0x14, 0xf6, 0x9e, 0x30, 0x67, 0x89,
        0x7a, 0xa8, 0x1a, 0x04, 0x0c, 0xb5, 0x59, 0xae, 0x49, 0x2e, 0x3a, 0x98, 0x1d, 0x14, 0xa9,
        0xb2, 0x46,
    ];

    fn test_dir(name: &str) -> PathBuf {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_millis();
        let dir = std::env::temp_dir().join(format!("desktop-pet-{name}-{millis}"));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn base_manifest() -> PetManifest {
        PetManifest {
            id: "chimmy".to_string(),
            name: "Chimmy".to_string(),
            status: "Descargada".to_string(),
            description: "Test pet".to_string(),
            preview_frame: "assets/idle.png".to_string(),
            idle_frame: "assets/idle.png".to_string(),
            active_frames: vec!["assets/active.png".to_string()],
            supports_skins: false,
            skins: vec![],
            source: None,
        }
    }

    fn write_test_assets(dir: &Path) -> ([u8; 32], [u8; 32]) {
        let assets_dir = dir.join("assets");
        fs::create_dir_all(&assets_dir).expect("create assets dir");
        let idle_path = assets_dir.join("idle.png");
        let active_path = assets_dir.join("active.png");
        fs::write(&idle_path, b"idle image bytes").expect("write idle");
        fs::write(&active_path, b"active image bytes").expect("write active");
        (
            sha256_digest(&idle_path).expect("idle digest"),
            sha256_digest(&active_path).expect("active digest"),
        )
    }

    fn sample_petpack_metadata(idle_hash: &str, active_hash: &str) -> PetpackMetadata {
        PetpackMetadata {
            schema_version: 2,
            package_id: "petpack_chimmy_1_0_0".to_string(),
            pet_id: "chimmy".to_string(),
            pet_version: "1.0.0".to_string(),
            minimum_app_version: "0.1.0".to_string(),
            manifest_path: "manifest.json".to_string(),
            license_path: "license.json".to_string(),
            assets: vec![
                PetpackAssetDigest {
                    path: "assets/idle.png".to_string(),
                    sha256: idle_hash.to_string(),
                },
                PetpackAssetDigest {
                    path: "assets/active.png".to_string(),
                    sha256: active_hash.to_string(),
                },
            ],
        }
    }

    fn sample_petpack_license() -> PetpackLicense {
        sample_petpack_license_for_account(DEVELOPMENT_ACCOUNT_ID)
    }

    fn sample_petpack_license_for_account(account_id: &str) -> PetpackLicense {
        PetpackLicense {
            license_id: "lic_test".to_string(),
            entitlement_id: "ent_test".to_string(),
            subject: LicenseSubject {
                subject_type: "account".to_string(),
                id: account_id.to_string(),
            },
            pet_id: "chimmy".to_string(),
            pet_version: "1.0.0".to_string(),
            issued_at: "2026-05-22T00:00:00Z".to_string(),
            expires_at: None,
            revalidate_after: None,
        }
    }

    fn write_v2_metadata(dir: &Path, idle_hash: &str, active_hash: &str) {
        write_v2_metadata_for_account(dir, idle_hash, active_hash, DEVELOPMENT_ACCOUNT_ID);
    }

    fn write_v2_metadata_for_account(
        dir: &Path,
        idle_hash: &str,
        active_hash: &str,
        account_id: &str,
    ) {
        let petpack = sample_petpack_metadata(idle_hash, active_hash);
        let license = sample_petpack_license_for_account(account_id);
        fs::write(
            dir.join(PETPACK_METADATA_FILE),
            serde_json::to_string_pretty(&petpack).expect("serialize petpack metadata"),
        )
        .expect("write petpack metadata");
        fs::write(
            dir.join(PETPACK_LICENSE_FILE),
            serde_json::to_string_pretty(&license).expect("serialize license metadata"),
        )
        .expect("write license metadata");

        let canonical_payload = canonical_signature_payload(&petpack, &license).expect("payload");
        let signing_key = SigningKey::from_bytes(&PETPACK_TEST_SECRET_KEY_BYTES);
        let signature = signing_key.sign(&canonical_payload);
        fs::write(
            dir.join(PETPACK_SIGNATURE_FILE),
            BASE64_STANDARD.encode(signature.to_bytes()),
        )
        .expect("write signature");
    }

    #[test]
    fn canonical_signature_payload_is_stable_jcs_json() {
        let petpack = sample_petpack_metadata("abc", "def");
        let license = sample_petpack_license();

        let payload = canonical_signature_payload(&petpack, &license).expect("canonical payload");
        let payload = String::from_utf8(payload).expect("utf8 payload");

        assert_eq!(
            payload,
            r#"{"license":{"entitlementId":"ent_test","expiresAt":null,"issuedAt":"2026-05-22T00:00:00Z","licenseId":"lic_test","petId":"chimmy","petVersion":"1.0.0","revalidateAfter":null,"subject":{"id":"user_test","type":"account"}},"petpack":{"assets":[{"path":"assets/idle.png","sha256":"abc"},{"path":"assets/active.png","sha256":"def"}],"licensePath":"license.json","manifestPath":"manifest.json","minimumAppVersion":"0.1.0","packageId":"petpack_chimmy_1_0_0","petId":"chimmy","petVersion":"1.0.0","schemaVersion":2}}"#
        );
    }

    #[test]
    fn digest_matches_hex_and_base64() {
        let digest = [0xde, 0xad, 0xbe, 0xef];
        assert!(digest_matches_declared(&digest, "DEADBEEF"));
        assert!(digest_matches_declared(
            &digest,
            &BASE64_STANDARD.encode(digest)
        ));
        assert!(!digest_matches_declared(&digest, "00000000"));
    }

    #[test]
    fn petpack_v1_without_metadata_skips_hash_validation() {
        let dir = test_dir("v1-fallback");
        let manifest = base_manifest();

        let result = validate_petpack_v2_metadata(&dir, &dir, &manifest, DEVELOPMENT_ACCOUNT_ID);

        let _ = fs::remove_dir_all(&dir);
        assert!(result.is_ok());
    }

    #[test]
    fn petpack_v2_accepts_hex_asset_hashes() {
        let dir = test_dir("hex-hashes");
        let manifest = base_manifest();
        let (idle_digest, active_digest) = write_test_assets(&dir);
        write_v2_metadata(&dir, &encode_hex(&idle_digest), &encode_hex(&active_digest));

        let result = validate_petpack_v2_metadata(&dir, &dir, &manifest, DEVELOPMENT_ACCOUNT_ID);

        let _ = fs::remove_dir_all(&dir);
        assert!(result.is_ok());
    }

    #[test]
    fn petpack_v2_accepts_base64_asset_hashes() {
        let dir = test_dir("base64-hashes");
        let manifest = base_manifest();
        let (idle_digest, active_digest) = write_test_assets(&dir);
        write_v2_metadata(
            &dir,
            &BASE64_STANDARD.encode(idle_digest),
            &BASE64_STANDARD.encode(active_digest),
        );

        let result = validate_petpack_v2_metadata(&dir, &dir, &manifest, DEVELOPMENT_ACCOUNT_ID);

        let _ = fs::remove_dir_all(&dir);
        assert!(result.is_ok());
    }

    #[test]
    fn petpack_v2_rejects_asset_hash_mismatch() {
        let dir = test_dir("hash-mismatch");
        let manifest = base_manifest();
        let (idle_digest, _) = write_test_assets(&dir);
        write_v2_metadata(&dir, &encode_hex(&idle_digest), "00000000");

        let result = validate_petpack_v2_metadata(&dir, &dir, &manifest, DEVELOPMENT_ACCOUNT_ID);

        let _ = fs::remove_dir_all(&dir);
        assert_eq!(result, Err("petpack asset hash mismatch".to_string()));
    }

    #[test]
    fn petpack_v2_rejects_invalid_signature() {
        let dir = test_dir("invalid-signature");
        let manifest = base_manifest();
        let (idle_digest, active_digest) = write_test_assets(&dir);
        write_v2_metadata(&dir, &encode_hex(&idle_digest), &encode_hex(&active_digest));
        fs::write(
            dir.join(PETPACK_SIGNATURE_FILE),
            BASE64_STANDARD.encode([0_u8; 64]),
        )
        .expect("overwrite signature");

        let result = validate_petpack_v2_metadata(&dir, &dir, &manifest, DEVELOPMENT_ACCOUNT_ID);

        let _ = fs::remove_dir_all(&dir);
        assert_eq!(
            result,
            Err("petpack signature verification failed".to_string())
        );
    }

    #[test]
    fn petpack_v2_rejects_other_account_license() {
        let dir = test_dir("other-account");
        let manifest = base_manifest();
        let (idle_digest, active_digest) = write_test_assets(&dir);
        write_v2_metadata_for_account(
            &dir,
            &encode_hex(&idle_digest),
            &encode_hex(&active_digest),
            "other_user",
        );

        let result = validate_petpack_v2_metadata(&dir, &dir, &manifest, DEVELOPMENT_ACCOUNT_ID);

        let _ = fs::remove_dir_all(&dir);
        assert_eq!(
            result,
            Err("petpack license belongs to another account".to_string())
        );
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_google_auth::init())
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

            // Hide main window on close instead of destroying it
            let main_window = app
                .get_webview_window("main")
                .ok_or_else(|| tauri::Error::Io(std::io::Error::other("main window not found")))?;
            let main_window_clone = main_window.clone();
            main_window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = main_window_clone.hide();
                }
            });

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
            get_current_account,
            save_account_session,
            clear_account_session,
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
