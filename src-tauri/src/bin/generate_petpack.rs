use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

const PETPACK_METADATA_FILE: &str = "petpack.json";
const PETPACK_LICENSE_FILE: &str = "license.json";
const PETPACK_SIGNATURE_FILE: &str = "signature.ed25519";
const PETPACK_MANIFEST_FILE: &str = "manifest.json";

// Development-only key matching the desktop app placeholder public key.
// Replace with server-side key management before production.
const DEV_SECRET_KEY_BYTES: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];

#[derive(Deserialize)]
struct PetSkinCatalogItem {
    id: String,
    name: String,
    price: u64,
    description: String,
}

#[derive(Deserialize)]
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
}

#[derive(Serialize)]
struct PetpackAssetDigest {
    path: String,
    sha256: String,
}

#[derive(Serialize)]
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

#[derive(Serialize)]
struct LicenseSubject {
    #[serde(rename = "type")]
    subject_type: String,
    id: String,
}

#[derive(Serialize)]
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse()?;
    let manifest_path = args.source_dir.join(PETPACK_MANIFEST_FILE);
    let manifest = read_manifest(&manifest_path)?;
    validate_manifest(&manifest)?;
    let pet_version = "1.0.0".to_string();
    let assets = collect_runtime_assets(&args.source_dir, &manifest)?;

    let petpack = PetpackMetadata {
        schema_version: 2,
        package_id: format!("petpack_{}_{}", manifest.id, pet_version.replace('.', "_")),
        pet_id: manifest.id.clone(),
        pet_version: pet_version.clone(),
        minimum_app_version: "0.1.0".to_string(),
        manifest_path: PETPACK_MANIFEST_FILE.to_string(),
        license_path: PETPACK_LICENSE_FILE.to_string(),
        assets,
    };
    let license = PetpackLicense {
        license_id: args.license_id,
        entitlement_id: args.entitlement_id,
        subject: LicenseSubject {
            subject_type: "account".to_string(),
            id: args.account_id,
        },
        pet_id: manifest.id,
        pet_version,
        issued_at: issued_at_now(),
        expires_at: None,
        revalidate_after: None,
    };

    let signature = sign_payload(&petpack, &license)?;
    write_petpack(
        &args.source_dir,
        &args.output_file,
        &petpack,
        &license,
        &signature,
    )?;
    println!("generated {}", args.output_file.display());
    Ok(())
}

struct Args {
    source_dir: PathBuf,
    output_file: PathBuf,
    account_id: String,
    entitlement_id: String,
    license_id: String,
}

impl Args {
    fn parse() -> Result<Self, String> {
        let mut args = env::args().skip(1);
        let source_dir = args.next().map(PathBuf::from).ok_or_else(usage)?;
        let output_file = args.next().map(PathBuf::from).ok_or_else(usage)?;
        let account_id = args.next().ok_or_else(usage)?;
        let entitlement_id = args.next().unwrap_or_else(|| "ent_dev".to_string());
        let license_id = args.next().unwrap_or_else(|| "lic_dev".to_string());

        if !source_dir.is_dir() {
            return Err("source_dir must be a directory".to_string());
        }

        Ok(Self {
            source_dir,
            output_file,
            account_id,
            entitlement_id,
            license_id,
        })
    }
}

fn usage() -> String {
    "usage: cargo run --manifest-path src-tauri/Cargo.toml --bin generate_petpack -- <source_dir> <output.petpack> <account_id> [entitlement_id] [license_id]".to_string()
}

fn read_manifest(path: &Path) -> Result<PetManifest, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str::<PetManifest>(&content)?)
}

fn validate_manifest(manifest: &PetManifest) -> Result<(), Box<dyn std::error::Error>> {
    if !is_valid_pet_id(&manifest.id) {
        return Err("manifest id must be a safe slug".into());
    }
    if manifest.name.trim().is_empty() {
        return Err("manifest name is required".into());
    }
    if manifest.status.trim().is_empty() {
        return Err("manifest status is required".into());
    }
    if manifest.description.trim().is_empty() {
        return Err("manifest description is required".into());
    }
    if !manifest.supports_skins && !manifest.skins.is_empty() {
        return Err("manifest skins must be empty when supportsSkins is false".into());
    }
    for skin in &manifest.skins {
        if skin.id.trim().is_empty()
            || skin.name.trim().is_empty()
            || skin.description.trim().is_empty()
        {
            return Err("manifest skin metadata has required empty fields".into());
        }
        let _ = skin.price;
    }
    Ok(())
}

fn is_valid_pet_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
}

fn collect_runtime_assets(
    source_dir: &Path,
    manifest: &PetManifest,
) -> Result<Vec<PetpackAssetDigest>, Box<dyn std::error::Error>> {
    let mut asset_paths = BTreeSet::new();
    asset_paths.insert(manifest.preview_frame.clone());
    asset_paths.insert(manifest.idle_frame.clone());
    asset_paths.extend(manifest.active_frames.iter().cloned());

    asset_paths
        .into_iter()
        .map(|path| {
            validate_relative_path(&path)?;
            let digest = sha256_hex(&source_dir.join(&path))?;
            Ok(PetpackAssetDigest {
                path,
                sha256: digest,
            })
        })
        .collect()
}

fn validate_relative_path(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("manifest asset path must be relative and stay inside the pet folder".into());
    }
    Ok(())
}

fn sha256_hex(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    let digest = Sha256::digest(bytes);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn sign_payload(
    petpack: &PetpackMetadata,
    license: &PetpackLicense,
) -> Result<String, Box<dyn std::error::Error>> {
    let payload = json!({
        "license": license,
        "petpack": petpack,
    });
    let canonical_payload = serde_json_canonicalizer::to_vec(&payload)?;
    let signing_key = SigningKey::from_bytes(&DEV_SECRET_KEY_BYTES);
    let signature = signing_key.sign(&canonical_payload);
    Ok(BASE64_STANDARD.encode(signature.to_bytes()))
}

fn write_petpack(
    source_dir: &Path,
    output_file: &Path,
    petpack: &PetpackMetadata,
    license: &PetpackLicense,
    signature: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = output_file.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let file = fs::File::create(output_file)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);

    zip.start_file(PETPACK_MANIFEST_FILE, options)?;
    let mut manifest_file = fs::File::open(source_dir.join(PETPACK_MANIFEST_FILE))?;
    io::copy(&mut manifest_file, &mut zip)?;

    for asset in &petpack.assets {
        zip.start_file(&asset.path, options)?;
        let mut asset_file = fs::File::open(source_dir.join(&asset.path))?;
        io::copy(&mut asset_file, &mut zip)?;
    }

    zip.start_file(PETPACK_METADATA_FILE, options)?;
    zip.write_all(serde_json::to_string_pretty(petpack)?.as_bytes())?;
    zip.start_file(PETPACK_LICENSE_FILE, options)?;
    zip.write_all(serde_json::to_string_pretty(license)?.as_bytes())?;
    zip.start_file(PETPACK_SIGNATURE_FILE, options)?;
    zip.write_all(signature.as_bytes())?;
    zip.finish()?;
    Ok(())
}

fn issued_at_now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    format!("unix:{seconds}")
}
