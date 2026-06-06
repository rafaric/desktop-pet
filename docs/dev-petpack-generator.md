# Development petpack v2 generator

This tool creates a signed `.petpack` v2 archive from a local pet folder so the desktop app can test the commercial validation path before the backend exists.

## Usage

Before running the generator, set `DESKTOP_PET_SIGNING_SECRET_KEY_HEX` in your local environment. The value must be a 32-byte Ed25519 secret key encoded as 64 hex characters.

From the repository root:

```bash
npm run petpack:dev -- <source_dir> <output.petpack> <account_id> [entitlement_id] [license_id]
```

Example:

```bash
npm run petpack:dev -- ./public/pets/demo ./tmp/chimmy-v2.petpack user_test ent_chimmy lic_chimmy
```

The source folder must contain:

```text
manifest.json
assets referenced by manifest.json
```

## Output

The generated archive includes:

```text
manifest.json
assets...
petpack.json
license.json
signature.ed25519
```

`petpack.json` contains SHA-256 hashes for runtime assets referenced by:

- `previewFrame`
- `idleFrame`
- `activeFrames`

`license.json` is account-bound to the provided `account_id`.

`signature.ed25519` signs the RFC 8785/JCS canonical payload:

```json
{
  "license": { "...": "license metadata" },
  "petpack": { "...": "package metadata" }
}
```

## Keys

This tool reads the Ed25519 private signing key from the local environment. The matching public key is embedded in the desktop app.

**Keep the signing key private.** For production, use server-side secrets management (env vars, HSM, Vault, etc.) and never commit the real private key to the repository.

Current public key embedded in the app (base64):

```
RItbG2jKB6NFa+dwLMHiAz3fn4vbwX/yL3GopqjtGok=
```

(hex: `448baffae28e07a3456be7702cc1e2033ddf9f8bdbc17ff22f71a8a6a8ed1a89`)

## Important

If you rotate the signing key, update the public key in `src-tauri/src/lib.rs` and rebuild the app. Petpacks signed with the old key will stop validating.
