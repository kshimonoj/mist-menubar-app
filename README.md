# Mist Menubar

A macOS menubar app that shows live **Juniper Mist** operational status in a
popover: SLE health, device up/down counts, client counts, and active alarms —
for a whole org or a single site. Built with **Tauri v2** (Rust backend +
vanilla HTML/JS frontend).

![scope](docs/scope.png)

## Features

- Lives in the **menubar** (no Dock icon).
- Click the tray icon → a popover opens **right below the icon**; it hides when
  it loses focus.
- **SLE** (Service Level Expectation) with **Wireless / Wired / WAN tabs**, each
  listing its individual metrics (Coverage, Roaming, Time to Connect, …) as a
  color-coded bar + percentage.
- **Devices**: AP / Switch / Gateway — `connected / total` with down highlighting.
- **Clients**: wireless & wired counts.
- **Alarms**: active count over the last 24h with a severity breakdown.
- **Scope switch**: whole org (default) or a specific site.
- **Open Mist Dashboard** in your default browser for the current scope.
- In-app **settings**: cluster (API host), API token, org & site selection,
  refresh interval. Auto-polls every 30 / 60 / 300 s.
- The tray title shows a short summary (worst SLE % and any down devices).

## Requirements

- macOS 13 (Ventura) or newer.
- A Mist **API token** (Mist account → My Account → API Tokens).

## Setup (development)

```bash
cd ~/dev/mist-menubar
npm install
npm run tauri dev      # run in dev mode
```

## Build a distributable

```bash
npm run tauri build
```

Artifacts land in:

- App: `src-tauri/target/release/bundle/macos/Mist.app`
- Disk image: `src-tauri/target/release/bundle/dmg/Mist_0.1.0_aarch64.dmg`

## Configuration (first run)

1. Launch the app — a small signal icon appears in the menubar.
2. Click it, then **Open Settings**.
3. Choose your **Cluster** (API host). Default candidate: `Global 03
   (api.ac2.mist.com)`. Pick **Custom…** to type any host.
4. Paste your **API Token** and click **Test Connection**. On success the org
   list is populated.
5. Pick an **Organization** (and optionally a **Site** — leave as "Org (all
   sites)" for the org-wide summary).
6. Choose a **Refresh interval** and click **Save**.

Settings persist across restarts (stored under
`~/Library/Application Support/com.mist.menubar/config.json`).

> The API token is stored locally in that config file. It is never bundled into
> the app, never sent anywhere except the Mist API host you configure, and never
> committed to source control.

## Distributing to others (unsigned)

This app is **not code-signed or notarized** (personal-distribution build), so
Gatekeeper will warn the first time. Just hand someone the `.dmg`.

To open it the first time, the recipient can either:

- **Right-click** `Mist.app` → **Open** → confirm **Open** in the dialog, or
- Remove the quarantine flag from Terminal:

  ```bash
  xattr -dr com.apple.quarantine /Applications/Mist.app
  ```

(Drag `Mist.app` from the mounted `.dmg` into `/Applications` first.)

## How it talks to Mist

All HTTP runs in the Rust backend (via `reqwest`, 10 s timeout) so the token
never touches the WebView and there are no CORS issues. Base URL is
`https://{api_host}/api/v1`, auth header `Authorization: Token {token}`.
Endpoints used: `/self`, `/orgs/{id}/sites`, `/orgs/{id}/insights/stats`,
`.../stats/devices`, client stats, and `.../alarms/search`. Calls within a
refresh cycle run in parallel.

Site SLE is fetched per metric from:

```
GET /sites/{id}/sle/site/{id}/metric/{metric}/summary?duration=1d
```

The success rate is computed from the returned `sle.samples` block as
`(1 - degraded / total) * 100`. All metrics are fetched concurrently; metric
names that vary by version are retried under alternate keys (e.g.
`switch-throughput` → `wired-throughput`, `wan-edge-health` → `gateway-health`).

## Known limitations

- **Metric/stat availability varies by Mist license tier and cluster version.**
  Endpoints or specific SLE metrics that aren't available (404 / error) are
  skipped and shown as **N/A** rather than failing the whole view. The SLE tabs
  only show meaningful values for a **site** scope; the org scope shows a single
  heuristic Health value per category.
- Org-level SLE is extracted heuristically from `/insights/stats`, whose shape
  differs between versions; if no health values are found it shows **N/A**.
  (The raw top-level keys are logged to stderr to aid tuning.)
- On HTTP 429 (rate limit) the current cycle is skipped until the next poll.
- Unsigned build — see the Gatekeeper note above.
- Built for Apple Silicon (`aarch64`). For Intel, build on / target `x86_64`.
