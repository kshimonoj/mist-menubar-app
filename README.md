# Mist Menubar

A macOS menu bar app for monitoring Juniper Mist network status.

## Features

- **SLE** (Service Level Expectation): Wireless / Wired / WAN metrics with per-metric breakdown
- **Devices**: AP / Switch / Gateway connected/total count
- **Clients**: Wireless / Wired client count
- **Alerts**: Last 7 days, with severity and summary text
- Org-wide or per-site scope switching
- One-click link to Mist Dashboard
- Auto-refresh (30s / 1min / 5min / 60min)

## Requirements

- macOS 13.0 or later
- Apple Silicon (M1/M2/M3) — for Intel Mac, rebuild with `--target x86_64-apple-darwin`
- Juniper Mist API token (read-only is sufficient)

## Installation

1. Download `Mist_x.x.x_aarch64.dmg`
2. Open the DMG and drag `Mist.app` to `/Applications/`
3. **First launch — Gatekeeper workaround** (required for unsigned apps):
   - Right-click `Mist.app` → "Open" → "Open" in the dialog, or
   - Run in Terminal:
     ```bash
     xattr -dr com.apple.quarantine /Applications/Mist.app
     ```
4. Click the tray icon in the menu bar to open the dashboard

## Setup

1. Click the ⚙ gear icon
2. Select your cluster (e.g. `api.ac2.mist.com`)
3. Enter your API token → click **Test Connection**
4. Select Org (and optionally a Site)
5. Click **Save**

## Build from Source

```bash
npm install
npm run tauri build
```

## Notes

- This app is **not notarized** (personal distribution only). Use the `xattr` command above on first launch.
- Some metrics may show N/A depending on your Mist license and cluster version.
- Intel Mac support requires a separate build.

## License

MIT
