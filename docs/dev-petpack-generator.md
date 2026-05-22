# Development petpack v2 generator

This tool creates a signed `.petpack` v2 archive from a local pet folder so the desktop app can test the commercial validation path before the backend exists.

## Usage

From the repository root:

```bash
npm run petpack:dev -- <source_dir> <output.petpack> <account_id> [entitlement_id] [license_id]
```

Example:

```bash
npm run petpack:dev -- C:/proyectos/miMascota C:/proyectos/chimmy-v2.petpack user_test ent_chimmy lic_chimmy
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

## Important

This is a development-only tool.

It uses the same RFC/test-vector private key that matches the desktop app's temporary embedded public key. Production must replace this with server-side key management and never ship a signing key in the app or repo.
