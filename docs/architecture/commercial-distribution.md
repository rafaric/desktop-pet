# Commercial pet distribution architecture

This document defines how the desktop pet app should sell, download, verify, and install paid pets without relying on unrestricted local asset import.

## Decision summary

| Area             | Decision                                                            |
| ---------------- | ------------------------------------------------------------------- |
| Commerce surface | A web landing/store in this repository under `web/`.                |
| Desktop role     | Runtime, installer, and validator for pets; not the primary store.  |
| MVP protection   | User account + server signature. No perfect DRM target.             |
| Pet delivery     | Server-generated `.petpack` files bound to a purchased entitlement. |
| Offline behavior | A valid installed pet can run offline after install.                |
| Dev imports      | Folder import and unsigned petpack import are development-only.     |
| Provider choice  | Auth and payment providers stay abstract for now.                   |

## Goals

- Let users download the desktop app for Windows and macOS from a simple landing page.
- Let logged-in users browse and download pets from a web store.
- Generate petpacks only for users who own the corresponding entitlement.
- Let the desktop app verify a petpack before installing it.
- Keep installed pets usable offline after successful validation.
- Avoid pretending the desktop app can provide perfect DRM.

## Non-goals

- Perfect copy protection.
- In-app payments in the desktop app for the MVP.
- Device-locked licenses for the first commercial version.
- Strong online revocation for the MVP.
- Replacing the current local prototype workflow immediately.

## System components

| Component             | Responsibility                                                                   |
| --------------------- | -------------------------------------------------------------------------------- |
| Landing/store web app | Marketing, app downloads, pet catalog, login, purchase/download UX.              |
| Auth provider         | Account identity for web and desktop. Abstract in the first design.              |
| Commerce backend      | Records purchases, entitlements, and download permissions.                       |
| Petpack service       | Assembles or streams user-bound signed `.petpack` files.                         |
| Object storage/CDN    | Stores immutable pet asset bundles and app installers.                           |
| Desktop app           | Logs into the same account, verifies petpacks, installs pets, runs pets offline. |

## Happy path

```text
User opens landing
→ downloads desktop app for Windows/macOS
→ logs into web store
→ buys or selects an owned pet
→ requests pet download
→ backend checks entitlement
→ backend generates a signed user-bound petpack
→ user imports/downloads it in the desktop app
→ desktop app verifies signature, account, and asset hashes
→ pet is installed locally
→ pet can be used offline
```

## Trust model

The server is the source of truth for purchases and entitlement issuance.

The desktop app is trusted only to verify:

1. the package was signed by the server;
2. the package was not modified;
3. the package is for the signed-in account;
4. the package metadata matches the pet manifest and assets.

The private signing key must never ship with the desktop app. The desktop app embeds the public verification key so petpacks can be validated offline after download. The signing key lives server-side (or in a secrets manager). Key rotation is supported by updating the embedded public key and regenerating petpacks with the new key. For local development, provide the private key through a local environment variable such as `DESKTOP_PET_SIGNING_SECRET_KEY_HEX`; it must never be committed.

## Petpack validation order

The desktop app should validate a commercial petpack in this order:

1. Extract to a temporary directory with zip-slip protection.
2. Require exactly one `petpack.json`, one `license.json`, one `signature.ed25519`, and one runtime `manifest.json`.
3. Parse `petpack.json`, `license.json`, and `manifest.json`.
4. Verify the server signature using the embedded public key.
5. Verify the signed license subject matches the current desktop account.
6. Verify pet id/version consistency across metadata and manifest.
7. Verify all declared asset hashes.
8. Verify app version compatibility.
9. Install into app data using staging/backup replacement.
10. Persist validated license metadata next to the installed pet.

## Account and offline policy

| Case                             | MVP behavior                                                                                                                                                                                    |
| -------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| User installs while signed in    | Allowed if license matches account.                                                                                                                                                             |
| User goes offline after install  | Installed pets keep working.                                                                                                                                                                    |
| User token expires after install | Installed pets keep working. Login needed only for new downloads/imports.                                                                                                                       |
| User signs out                   | Installed pets remain on disk; product decision needed whether to show/use them while signed out. Recommended MVP: continue showing already-installed pets, but require login for new installs. |
| User switches account            | New installs must match the new account. Existing pets can remain installed, but UI should eventually show ownership/account provenance.                                                        |

## Development vs production import boundary

Current folder import and unsigned petpack import are useful for development. They should not be exposed as the commercial flow.

Recommended policy:

| Build mode  | Folder import                  | Unsigned petpack import | Signed licensed petpack import |
| ----------- | ------------------------------ | ----------------------- | ------------------------------ |
| Development | Allowed                        | Allowed                 | Allowed when implemented       |
| Production  | Hidden and rejected by backend | Rejected by backend     | Required                       |

The Rust backend must enforce this boundary. Hiding the UI is not enough because Tauri commands can be invoked directly if exposed.

Suggested build flags:

```text
DESKTOP_PET_DEV_IMPORTS=true       # Rust/backend compile-time gate
VITE_DESKTOP_PET_DEV_IMPORTS=true  # Vite/frontend visibility gate
```

Production builds should default both to false. The backend flag is the security boundary; the frontend flag only controls whether the development import UI is visible.

## Store API shape

Provider-neutral API contract for the future web/backend is drafted in:

```text
services/store-api/openapi.yaml
```

Initial endpoint shape:

| Endpoint                      | Purpose                                                               |
| ----------------------------- | --------------------------------------------------------------------- |
| `GET /pets`                   | Public/commercial pet catalog.                                        |
| `GET /app-downloads`          | Windows/macOS desktop installer metadata.                             |
| `GET /me/library`             | Pets owned by the signed-in user.                                     |
| `POST /checkout/pets/:petId`  | Start purchase flow with the selected payment provider.               |
| `POST /webhooks/payment`      | Receive purchase confirmation and create entitlement.                 |
| `POST /downloads/pets/:petId` | Check entitlement and generate a licensed petpack download.           |
| `GET /downloads/:downloadId`  | Stream the generated petpack or redirect to a short-lived signed URL. |

## Petpack generation strategy

Use a hybrid build model:

1. Build immutable pet assets once per pet version.
2. Store the base package or asset bundle in object storage.
3. On download, generate user-bound license metadata.
4. Sign the metadata and asset digest list.
5. Return a single `.petpack` file for simple UX.

This avoids rebuilding image assets for every buyer while still producing a user-bound package.

## MVP phases

### Phase 1 — Architecture/specs

Done when:

- commercial distribution architecture exists;
- petpack v2 format exists;
- dev/prod import boundary is documented.

### Phase 2 — Landing/store skeleton

Done when:

- `web/` has landing routes;
- app download placeholders exist for Windows/macOS;
- pet catalog page exists;
- auth/payment are still mocked or provider-neutral.

### Phase 3 — Entitlements and downloads

Done when:

- backend can represent owned pets;
- download endpoint can issue a licensed petpack for an owned pet;
- unauthorized downloads are rejected.

### Phase 4 — Desktop account session

Current development bridge:

- desktop app exposes a temporary `user_test` account placeholder;
- petpack v2 license subject must match that placeholder before install;
- this is not real authentication and must be replaced before production.

Done when:

- desktop app can sign in or receive a browser login callback;
- current account identity is visible in the app;
- tokens are stored securely through OS credential storage.

### Phase 5 — Signed petpack verification

Done when:

- desktop app verifies signatures and hashes;
- license subject must match current desktop account;
- valid pets install and run offline.

### Phase 6 — Production import gating

Done when:

- folder import is hidden in production;
- Rust commands reject dev-only imports in production;
- only signed licensed petpacks are accepted for commercial installs.

## Hardening backlog

Required before public launch:

- revisit petpack zip limits for production thresholds (current extractor has entry, per-file, and total extracted size limits);
- clearer user-facing import errors;
- signing key management plan;
- app update and minimum supported app version behavior;
- Windows code signing;
- macOS notarization;
- download rate limiting;
- audit logs for downloads;
- support copy for license mismatch errors.

Later:

- optional soft revalidation window;
- revocation strategy;
- direct “open in app” links from the store;
- device-bound licenses if account-bound sharing becomes a real issue.

## Open decisions

- Whether installed pets should be usable after explicit logout.
- Which auth provider to use.
- Which payment provider to use.
- Whether the first store release supports manual petpack import or only direct download/open-in-app.
- How much ownership provenance to show in the local pet library.
