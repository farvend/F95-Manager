# F95 Standalone Client

Desktop client for F95zone — browse, search, download, and manage games from a native app.

Built with Rust + egui for speed and small filesize.

## Features

- Search just like on F95 with all its features:
  - In-app browsing with powerful search, tag and prefix filters, and mode switches.
- Effortless installs:
  - Press Download — the launcher fetches the archive and unzips the game for you, no hassle.
- Library:
  - Keep an installed library where you can run a game, reveal its folder, or delete it from disk.
- Custom launching options:
  - Want to run games in a sandbox? Open Settings → Custom launch command and enter something like:
    - `"C:\Program Files\Sandboxie-Plus\Start.exe" /box:1 {{path}}`
    - You don’t replace `{{path}}`; it’s a placeholder the manager will substitute with the game’s exe path.
- Configurable folders:
  - Set Temp (download), Extract-to (install), and Cache folders in Settings. Changing Extract-to moves installed games automatically.
- Portable and local by default:
  - No installer. Everything (settings, caches, installed games) lives next to the executable by default.
- Visual warnings:
  - Optional warning badges on cards for selected tags/prefixes.
  - Configure in Settings → Warn tags / Warn prefixes: pick items you want highlighted.
  - A red square on each game card shows the total number of warnings; hover it to see a list grouped by Tags/Prefixes.
  - Visual-only hint — it does not block or filter games; use Search filters to include/exclude content.
- Logging and error UI:
  - Built-in logs and error panels help understand what happened during downloads or launches.

---

## Download & Run

Requirements:
- Windows 10+ (other versions might work, untested).

1. Get the latest release: https://github.com/farvend/F95-Manager/releases/latest  
2. Download `F95_manager.exe` (or the latest `F95_manager-vX.Y.Z.exe`).
3. Run the executable.
4. Authorize (login or paste cookies) to access F95 content (see “Authorization” below).
5. Search, press Download, wait for extraction, then Play from your Library.

Tip: The app is portable. You can keep it on any drive and copy the whole folder elsewhere.

---

## Authorization

F95zone requires an authenticated session cookie. Two ways to provide it:

1) Username/Password  (MAY NOT WORK!!! IF SO, USE SECOND METHOD)  
   - Enter Username and Password on the Login screen.  
   - The app posts to `https://f95zone.to/login/login`, collects `Set-Cookie`, and saves a ready‑to‑use `Cookie` header string into `app_config.json`.  
   - If you get “login failed … xf_session” — check your credentials.

2) Paste a Cookie header string  
   - On the Login screen, paste the full `Cookie` header and press “Use cookies”.  
   - At minimum include `xf_session=…`. You may also have `xf_user`, `xf_csrf`, etc.

### How to get the Cookie header from your browser

Chrome/Edge/Chromium:
1. Log in to `https://f95zone.to`.
2. Open DevTools (F12) → Application → Cookies → `https://f95zone.to`.
3. Copy `name=value` pairs and join them with `; ` into one line.  
   - Alternative: Network tab → pick any request to `f95zone.to` → Request Headers → `Cookie`.

Firefox:
1. F12 → Storage → Cookies → `https://f95zone.to`.
2. Copy needed cookies and join into a single line, e.g.:  
   `xf_session=...; xf_user=...; xf_csrf=...`

Example:
```
xf_session=...; xf_user=...; xf_csrf=...
```

### Where it is stored

`app_config.json` (next to the executable, created on login):
```json
{
  "cookies": "xf_session=...; xf_user=...; xf_csrf=...",
  "username": "myname"
}
```
You can edit this file manually before launching. An empty/missing `cookies` value triggers the Login screen.

---

## Settings and Data Locations

All files are local to the app folder by default.

You can change Temp/Extract-to/Cache in Settings:
- If you change Extract-to and there are installed games, the app will prompt to move them automatically to the new folder and update the library records.

---

## Custom Launch Command ({{path}})

You can define a custom command to start games (e.g. in a sandbox or wrapper). The placeholder `{{path}}` will be replaced with the full path to the game’s executable.

Example (Sandboxie-Plus):
```
"C:\Program Files\Sandboxie-Plus\Start.exe" /box:1 {{path}}
```

Notes:
- Always keep quotes around the executable inside the template if paths can contain spaces. The app substitutes `{{path}}` with a quoted path.

---

## Warn Tags / Prefixes

Use warnings as lightweight content markers you control.

- Configure in Settings:
  - Warn tags: choose specific F95 tags to flag.
  - Warn prefixes: choose thread prefixes to flag (e.g., Abandoned, On Hold, etc.).
- How it looks:
  - Each card shows a small red square with the total number of matched items; hover it to see the names grouped under Tags and Prefixes.

---

## How Downloads Work

- The app parses the thread’s Downloads block, groups links by platform, and picks the match for your OS.
- If it can’t determine platform labels, you’ll be asked to pick a link from the page.
- Mirrors are tried in order until one succeeds. If a F95 requires a CAPTCHA you will be prompted to pass it.
- After download completes, the archive is extracted to the Extract-to folder and the game is added to your Library.
- The app tries to pick the best .exe near the root (ignoring common installers/uninstallers) and remembers it.

---

## Build From Source

1. Install Rust (stable): https://rustup.rs
2. Clone and build:
   ```
   git clone https://github.com/farvend/F95-Manager.git
   cd F95-Manager
   cargo build --release
   ```
3. The binary will be in `target/release/`. Place it alongside your data (or copy your existing `app_settings.json` / `app_config.json`) and run it.

---

## Troubleshooting

- Login: “xf_session” errors  
  Use the Cookie header method (recommended). Make sure `xf_session` is present and not expired.

---