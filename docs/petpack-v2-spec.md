# Petpack v2 commercial package spec

Petpack v2 is the commercial package format for paid pets. It keeps the current runtime `manifest.json`, then adds signed license and integrity metadata so the desktop app can validate a package before installation.

## Decision summary

| Area                | Decision                                                                                                        |
| ------------------- | --------------------------------------------------------------------------------------------------------------- |
| Archive type        | Zip-compatible `.petpack`.                                                                                      |
| Runtime metadata    | Keep existing `manifest.json`.                                                                                  |
| Commercial metadata | Add `petpack.json`, `license.json`, and `signature.ed25519`.                                                    |
| Signature model     | Server signs canonical metadata with an asymmetric key. Desktop embeds the public key for offline verification. |
| License binding     | Account-bound for MVP. Device binding is out of scope.                                                          |
| Offline use         | Allowed after successful install validation.                                                                    |

## Archive layout

Recommended layout:

```text
chimmy.petpack
├─ petpack.json
├─ license.json
├─ signature.ed25519
├─ manifest.json
└─ assets/
   ├─ idle.png
   ├─ active-01.png
   └─ active-02.png
```

The app should reject packages with:

- missing `petpack.json`;
- missing `license.json`;
- missing `signature.ed25519`;
- missing `manifest.json`;
- more than one `petpack.json`, `license.json`, `signature.ed25519`, or runtime `manifest.json`;
- asset paths that escape the package root;
- undeclared or hash-mismatched runtime assets.

## Existing `manifest.json`

`manifest.json` remains focused on runtime behavior:

```json
{
  "id": "chimmy",
  "name": "Chimmy",
  "status": "Descargada",
  "description": "Mascota premium de ejemplo.",
  "previewFrame": "assets/idle.png",
  "idleFrame": "assets/idle.png",
  "activeFrames": ["assets/active-01.png", "assets/active-02.png"],
  "supportsSkins": false,
  "skins": [],
  "source": "imported"
}
```

Rules:

- `id` must be a safe slug.
- frame paths must be relative package paths.
- runtime asset paths must stay inside the extracted package root.
- commercial ownership data does not belong in this file.

## `petpack.json`

`petpack.json` describes the immutable package and its assets.

Example:

```json
{
  "schemaVersion": 2,
  "packageId": "petpack_chimmy_1_0_0",
  "petId": "chimmy",
  "petVersion": "1.0.0",
  "minimumAppVersion": "0.1.0",
  "manifestPath": "manifest.json",
  "licensePath": "license.json",
  "assets": [
    {
      "path": "assets/idle.png",
      "sha256": "base64-or-hex-sha256"
    },
    {
      "path": "assets/active-01.png",
      "sha256": "base64-or-hex-sha256"
    },
    {
      "path": "assets/active-02.png",
      "sha256": "base64-or-hex-sha256"
    }
  ]
}
```

Rules:

- `petId` must match `manifest.json.id`.
- `manifestPath` must point to the package runtime manifest.
- every runtime asset referenced by `manifest.json` must be listed in `assets`.
- extra files should either be rejected or explicitly listed, depending on final policy.

Recommended MVP: reject unexpected non-metadata files to keep validation simple.

## `license.json`

`license.json` binds the package to a purchased entitlement and account subject.

Example:

```json
{
  "licenseId": "lic_01HXAMPLE",
  "entitlementId": "ent_01HXAMPLE",
  "subject": {
    "type": "account",
    "id": "user_01HXAMPLE"
  },
  "petId": "chimmy",
  "petVersion": "1.0.0",
  "issuedAt": "2026-05-19T00:00:00Z",
  "expiresAt": null,
  "revalidateAfter": null
}
```

Rules:

- `subject.id` must match the currently signed-in desktop account at install time.
- `petId` and `petVersion` must match `petpack.json`.
- `expiresAt` is optional and should be `null` for the MVP unless there is a clear subscription model.
- `revalidateAfter` is optional and should be `null` for the first offline-friendly MVP.

## `signature.ed25519`

The server signs a canonical payload covering the package metadata and license.

MVP canonicalization rule:

1. Build a JSON object with exactly two top-level keys in this order: `license`, then `petpack`.
2. Use the parsed contents of `license.json` and `petpack.json` as the values.
3. Serialize as canonical JSON using RFC 8785 JSON Canonicalization Scheme (JCS).
4. Sign the resulting UTF-8 bytes with Ed25519.
5. Store the raw signature bytes in `signature.ed25519`, encoded as base64.

Canonical signed payload before JCS serialization:

```json
{
  "license": { "...": "license.json contents" },
  "petpack": { "...": "petpack.json contents" }
}
```

The desktop app must reconstruct the same canonical payload from the extracted files before verification. It must not verify by signing raw zip bytes or non-canonical pretty-printed JSON.

The signature must cover:

- pet id;
- pet version;
- minimum app version;
- asset paths and hashes;
- license id;
- entitlement id;
- account subject;
- issue/expiry/revalidation metadata.

The signature does not need to cover the raw zip bytes if every install-relevant file is represented by signed metadata and hash checks.

## Desktop verification algorithm

```text
extract to temp directory
→ reject unsafe paths
→ parse manifest.json
→ parse petpack.json
→ parse license.json
→ verify signature.ed25519
→ compare license subject with current account
→ compare pet ids and versions across files
→ verify minimum app version
→ verify all asset hashes
→ install with staging/backup replacement
→ persist validated license with installed pet
```

## Installed pet layout

After validation, the desktop app can install to app data:

```text
<AppData>/pets/chimmy/
├─ manifest.json
├─ petpack.json
├─ license.json
├─ signature.ed25519
└─ assets/
   ├─ idle.png
   ├─ active-01.png
   └─ active-02.png
```

Persisting license metadata next to the pet makes offline use simple and auditable.

## Error cases

| Case              | User-facing message                                     |
| ----------------- | ------------------------------------------------------- |
| Missing metadata  | `Este petpack no tiene el formato esperado.`            |
| Invalid signature | `No se pudo verificar este petpack.`                    |
| Account mismatch  | `Este petpack pertenece a otra cuenta.`                 |
| Hash mismatch     | `El petpack parece estar dañado o modificado.`          |
| App too old       | `Actualizá la app para instalar esta mascota.`          |
| Duplicate pet id  | `Esta mascota ya está instalada. ¿Querés reemplazarla?` |

## Security notes

- This is reasonable commercial protection, not perfect DRM.
- The private signing key must stay server-side.
- The app should include zip size/count limits before accepting untrusted packages at scale.
- Device binding is intentionally excluded from the MVP to reduce support friction.
- Obfuscation may be added later, but signature and entitlement validation are the real foundation.

## Compatibility with current prototype

Current `.petpack` files are simple zip archives containing `manifest.json` and assets. They are useful as development petpacks.

Commercial v2 petpacks add:

- `petpack.json`;
- `license.json`;
- `signature.ed25519`;
- account-bound validation;
- asset hash verification.

Production should eventually reject unsigned v1/dev petpacks unless a dev-import flag is explicitly enabled.
