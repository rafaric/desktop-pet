# Desktop Pet Companion

Tauri-based desktop pet companion for Windows and macOS.

## MVP direction

- One active desktop pet at a time.
- Always-on-top transparent pet overlay.
- Minimalist/cartoon visual style.
- Local activity points from mouse clicks and keyboard presses while a pet is active.
- Per-pet skins unlocked with local points.
- Pets are purchased on the web, downloaded into the app, and usable offline.

## First proof of concept

Validate the Tauri overlay experience:

- transparent borderless pet window;
- always-on-top behavior;
- tray/menu bar controls;
- position, size, opacity controls;
- non-intrusive focus behavior on Windows and macOS.

## Development

Install dependencies:

```bash
npm install
```

Run frontend-only development server:

```bash
npm run dev
```

Build frontend:

```bash
npm run build
```

Run Tauri desktop app:

```bash
npm run tauri dev
```

> Tauri desktop development requires Rust/Cargo and the Tauri OS prerequisites.
