# F95 Manager

Desktop client for F95zone - browse, download, and manage games from a native app.

Built with Rust + egui for speed and efficiency. Features include advanced search, 
automated downloads with extraction, and a built-in library to organize your collection.


## Install

Requirements:
- Windows 10+ (other versions might work, untested).

1. Go to https://github.com/farvend/F95-Manager/releases/latest
3. Download `F95_manager.exe`
4. Run the executable
5. Log in to your F95zone account

## Authorization

F95zone requires an authenticated session cookie. Two ways to provide it:

1) Username/Password  (MAY NOT WORK!!! IF SO, USE SECOND METHOD)
   - Enter Username and Password on the Login screen.  

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

## Disclaimer

This client does not bypass authorization and is not meant to circumvent paid access. You use your own session/cookies and are responsible for complying with F95zone rules and your local laws.
