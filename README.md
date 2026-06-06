# Desktop Pet Companion

Tauri-based desktop pet companion for Windows and macOS.

## Product snapshot

- One active desktop pet at a time.
- Always-on-top transparent pet overlay.
- Local activity points from mouse clicks and keyboard presses.
- Per-pet skins unlocked with local points.
- Web-purchased pets downloaded into the app as signed `.petpack` files.
- Spanish-first product copy, English code/config.

## Repository layout

- `src/` — React desktop UI.
- `src-tauri/` — Rust/Tauri app, tray, overlay, petpack validation.
- `services/backend/` — minimal local store backend and petpack download flow.
- `services/store-api/` — provider-neutral OpenAPI draft.
- `web/` — landing/store static site.
- `public/pets/` — bundled local demo pets.

## Local prerequisites

### All platforms

- Node.js 20+
- npm 10+
- Rust stable toolchain

### macOS

Install Apple build tools first:

```bash
xcode-select --install
```

Then follow the Tauri macOS prerequisites if your machine is fresh.

## Local secrets / environment

This repo no longer stores live OAuth or signing secrets.

1. Copy the values you need from `env.example.txt` into a local `.env` file at the repository root.
2. Provide these variables locally:
   - `VITE_GOOGLE_CLIENT_ID`
   - `VITE_GOOGLE_CLIENT_SECRET`
   - `DESKTOP_PET_SIGNING_SECRET_KEY_HEX`

Optional:

- `PETPACK_GENERATOR_BIN` — override the backend path to the petpack generator binary.
- `DESKTOP_PET_BACKEND_PORT` — override backend port (default `3001`).

## Run the desktop app

From the repository root:

```bash
npm install
npm run tauri dev
```

Build the frontend only:

```bash
npm run build
```

Build the desktop release bundle:

```bash
npm run tauri build
```

On macOS, this is the command you will use to produce the `.app` bundle locally.

## Run the backend

Install backend dependencies:

```bash
cd services/backend
npm install
```

Build the Rust petpack generator once:

```bash
npm run build:generator
```

Start the backend:

```bash
npm run dev
```

Notes:

- The backend resolves the generator binary cross-platform.
- It looks first at `PETPACK_GENERATOR_BIN`, then at `src-tauri/target/debug/`, then `src-tauri/target/release/`.
- The sample premium pet source now points at `public/pets/demo`, so the backend can run on a fresh clone without Windows-only paths.

## Run the web landing/store

From the repository root:

```bash
npm run web:dev
npm run web:build
```

## macOS-specific development notes

### Activity tracking permission

Global activity tracking uses `rdev`.
On macOS, the app (or the terminal launching it during development) must be granted **Accessibility** permission, otherwise keyboard/mouse tracking may silently fail.

### Tray / overlay behavior

The overlay, tray, always-on-top behavior, and close-to-hide behavior should be validated on a real Mac because these platform behaviors differ from Windows.

### Google OAuth

The Google login flow uses `tauri-plugin-google-auth` with a localhost redirect server.
Your Google OAuth desktop/web credentials must allow `http://localhost` redirects.

## Release work still expected on macOS

A fresh clone should now be good enough to continue development on a Mac, but production release still requires platform work:

- Apple code signing
- notarization
- optional `.dmg` packaging/distribution polish
- final macOS QA for tray, overlay, permissions, and login

## Current status

- Windows installers exist and are linked from `web/`.
- macOS distribution is not published yet.
- The repo is being prepared so development can continue from a Mac clone cleanly.
