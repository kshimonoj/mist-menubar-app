import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

// Cluster options: label + api host. "manage" host is derived backend-side.
const CLUSTERS = [
  { label: "Global 01 (api.mist.com)", host: "api.mist.com" },
  { label: "Global 02 (api.gc1.mist.com)", host: "api.gc1.mist.com" },
  { label: "Global 03 (api.ac2.mist.com)", host: "api.ac2.mist.com" },
  { label: "Global 04 (api.gc2.mist.com)", host: "api.gc2.mist.com" },
  { label: "EU 01 (api.eu.mist.com)", host: "api.eu.mist.com" },
  { label: "Custom…", host: "__custom__" },
];

// In-memory app state.
let cfg = {
  api_host: "api.ac2.mist.com",
  custom_host: "",
  token: "",
  org_id: "",
  org_name: "",
  site_id: "", // "" => whole org
  site_name: "",
  interval: 60,
};
let orgs = []; // [{org_id, name}]
let sites = []; // [{id, name}]
let pollTimer = null;
let loading = false;
let lastSle = { wireless: [], wired: [], wan: [] }; // [{name, key, value}]
let activeSleTab = "wireless";

const $ = (sel) => document.querySelector(sel);

// ---------- persistence (via Rust commands -> store) ----------
async function loadConfig() {
  try {
    const saved = await invoke("load_config");
    if (saved && typeof saved === "object") cfg = { ...cfg, ...saved };
  } catch (e) {
    console.error("loadConfig", e);
  }
}

async function saveConfig() {
  try {
    await invoke("save_config", { config: cfg });
  } catch (e) {
    console.error("saveConfig", e);
  }
}

function isConfigured() {
  return Boolean(cfg.token && effectiveHost() && cfg.org_id);
}

function effectiveHost() {
  return cfg.api_host === "__custom__" ? cfg.custom_host : cfg.api_host;
}

// ---------- view switching ----------
function showView(name) {
  $("#dashboard").classList.toggle("hidden", name !== "dashboard");
  $("#settings").classList.toggle("hidden", name !== "settings");
}

// ---------- color helpers ----------
function sleClass(pct) {
  if (pct == null || Number.isNaN(pct)) return "gray";
  if (pct >= 90) return "green";
  if (pct >= 80) return "orange";
  return "red";
}

// ---------- dashboard rendering ----------
function renderScopeHeader() {
  const label =
    cfg.site_id && cfg.site_name
      ? `Site: ${cfg.site_name}`
      : `Org: ${cfg.org_name || "—"}`;
  $("#scope-label").textContent = label;
}

function renderScopeSelect() {
  const sel = $("#scope-select");
  sel.innerHTML = "";
  const orgOpt = document.createElement("option");
  orgOpt.value = "";
  orgOpt.textContent = "Org (all)";
  sel.appendChild(orgOpt);
  for (const s of sites) {
    const o = document.createElement("option");
    o.value = s.id;
    o.textContent = s.name;
    sel.appendChild(o);
  }
  sel.value = cfg.site_id || "";
}

// Format an SLE percentage for display. Very-high values collapse to ">99%".
function fmtPct(pct) {
  if (pct == null || Number.isNaN(pct)) return "N/A";
  if (pct >= 99.5) return ">99%";
  return Math.round(pct) + "%";
}

// Render the SLE list for the currently-active tab (wireless/wired/wan).
function renderSle() {
  const list = $("#sle-list");
  if (!list) return;
  const metrics = lastSle?.[activeSleTab] || [];
  list.innerHTML = "";
  if (!metrics.length) {
    const empty = document.createElement("div");
    empty.className = "sle-empty";
    empty.textContent = "N/A";
    list.appendChild(empty);
    return;
  }
  for (const m of metrics) {
    const pct = m.value;
    const cls = sleClass(pct);
    const row = document.createElement("div");
    row.className = "metric-row";

    const name = document.createElement("span");
    name.className = "metric-name";
    name.textContent = m.name;

    const wrap = document.createElement("div");
    wrap.className = "bar-wrap";
    const bar = document.createElement("div");
    bar.className = "bar b-" + cls;
    bar.style.width =
      pct == null || Number.isNaN(pct)
        ? "0%"
        : Math.max(0, Math.min(100, pct)) + "%";
    wrap.appendChild(bar);

    const val = document.createElement("span");
    val.className = "metric-val c-" + cls;
    val.textContent = fmtPct(pct);

    row.appendChild(name);
    row.appendChild(wrap);
    row.appendChild(val);
    list.appendChild(row);
  }
}

function selectSleTab(tab) {
  activeSleTab = tab;
  for (const btn of document.querySelectorAll(".sle-tab")) {
    btn.classList.toggle("active", btn.dataset.tab === tab);
  }
  renderSle();
}

function setDevice(key, info) {
  const row = document.querySelector(`.dev-row[data-key="${key}"]`);
  if (!row) return;
  const valEl = row.querySelector(".dev-val");
  if (!info || info.total == null) {
    valEl.textContent = "N/A";
    valEl.className = "dev-val c-gray";
    return;
  }
  valEl.textContent = `${info.connected}/${info.total}`;
  valEl.className = "dev-val " + (info.down > 0 ? "c-red" : "c-green");
}

function setClients(key, n) {
  const row = document.querySelector(`.dev-row[data-key="${key}"]`);
  if (!row) return;
  const valEl = row.querySelector(".dev-val");
  if (n == null || Number.isNaN(n)) {
    valEl.textContent = "N/A";
    valEl.className = "dev-val c-gray";
  } else {
    valEl.textContent = String(n);
    valEl.className = "dev-val";
  }
}

function renderAlarms(alarms) {
  const totalEl = $("#alarms-total");
  const bd = $("#alarms-breakdown");
  bd.innerHTML = "";
  if (!alarms) {
    totalEl.textContent = "N/A";
    totalEl.className = "dev-val c-gray";
    return;
  }
  const total = alarms.total ?? 0;
  const sev = alarms.severities || {};
  const crit = (sev.critical || 0);
  const warn = (sev.warn || 0) + (sev.warning || 0);
  let cls = "c-green";
  if (crit > 0) cls = "c-red";
  else if (warn > 0) cls = "c-orange";
  totalEl.textContent = String(total);
  totalEl.className = "dev-val " + cls;
  for (const [k, v] of Object.entries(sev)) {
    if (!v) continue;
    const chip = document.createElement("span");
    chip.className = "sev-chip";
    chip.textContent = `${k}: ${v}`;
    bd.appendChild(chip);
  }
}

function trayTitleFrom(data) {
  // Worst SLE across every metric + down devices, short.
  const sle = data?.sle || {};
  const sles = ["wireless", "wired", "wan"]
    .flatMap((c) => sle[c] || [])
    .map((m) => m?.value)
    .filter((v) => v != null && !Number.isNaN(v));
  let down = 0;
  for (const k of ["ap", "switch", "gateway"]) {
    down += data?.devices?.[k]?.down || 0;
  }
  let parts = [];
  if (sles.length) parts.push(Math.min(...sles).toFixed(0) + "%");
  if (down > 0) parts.push("▼" + down);
  return parts.join(" ");
}

async function refresh() {
  if (loading || !isConfigured()) return;
  loading = true;
  $("#last-updated").textContent = "Loading…";
  try {
    const data = await invoke("get_dashboard", {
      host: effectiveHost(),
      token: cfg.token,
      orgId: cfg.org_id,
      siteId: cfg.site_id || null,
    });

    lastSle = {
      wireless: data?.sle?.wireless || [],
      wired: data?.sle?.wired || [],
      wan: data?.sle?.wan || [],
    };
    renderSle();

    setDevice("ap", data?.devices?.ap);
    setDevice("switch", data?.devices?.switch);
    setDevice("gateway", data?.devices?.gateway);

    setClients("wireless-clients", data?.clients?.wireless ?? null);
    setClients("wired-clients", data?.clients?.wired ?? null);

    renderAlarms(data?.alarms);

    const now = new Date();
    $("#last-updated").textContent =
      "Updated " +
      now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });

    // Update tray title.
    invoke("set_tray_title", { title: trayTitleFrom(data) }).catch(() => {});
  } catch (e) {
    console.error("refresh", e);
    $("#last-updated").textContent = "Error";
  } finally {
    loading = false;
  }
}

function startPolling() {
  if (pollTimer) clearInterval(pollTimer);
  const ms = (Number(cfg.interval) || 60) * 1000;
  pollTimer = setInterval(refresh, ms);
}

// ---------- settings UI ----------
function fillClusterSelect() {
  const sel = $("#cfg-cluster");
  sel.innerHTML = "";
  for (const c of CLUSTERS) {
    const o = document.createElement("option");
    o.value = c.host;
    o.textContent = c.label;
    sel.appendChild(o);
  }
  sel.value = cfg.api_host;
  toggleCustomHost();
}

function toggleCustomHost() {
  const isCustom = $("#cfg-cluster").value === "__custom__";
  $("#custom-host-field").classList.toggle("hidden", !isCustom);
}

function fillOrgSelect() {
  const sel = $("#cfg-org");
  sel.innerHTML = "";
  if (!orgs.length) {
    const o = document.createElement("option");
    o.value = "";
    o.textContent = "— test connection first —";
    sel.appendChild(o);
    return;
  }
  for (const org of orgs) {
    const o = document.createElement("option");
    o.value = org.org_id;
    o.textContent = org.name;
    sel.appendChild(o);
  }
  sel.value = cfg.org_id || orgs[0].org_id;
}

function fillSiteSelect() {
  const sel = $("#cfg-site");
  sel.innerHTML = "";
  const all = document.createElement("option");
  all.value = "";
  all.textContent = "Org (all sites)";
  sel.appendChild(all);
  for (const s of sites) {
    const o = document.createElement("option");
    o.value = s.id;
    o.textContent = s.name;
    sel.appendChild(o);
  }
  sel.value = cfg.site_id || "";
}

function loadSettingsForm() {
  fillClusterSelect();
  $("#cfg-custom-host").value = cfg.custom_host || "";
  $("#cfg-token").value = cfg.token || "";
  $("#cfg-interval").value = String(cfg.interval || 60);
  fillOrgSelect();
  fillSiteSelect();
  $("#test-result").textContent = "";
  $("#save-result").textContent = "";
}

async function testConnection() {
  const btn = $("#btn-test");
  const out = $("#test-result");
  const host =
    $("#cfg-cluster").value === "__custom__"
      ? $("#cfg-custom-host").value.trim()
      : $("#cfg-cluster").value;
  const token = $("#cfg-token").value.trim();
  if (!host || !token) {
    out.className = "test-result err";
    out.textContent = "Host and token are required.";
    return;
  }
  btn.disabled = true;
  out.className = "test-result";
  out.textContent = "Testing…";
  try {
    orgs = await invoke("get_self", { host, token });
    if (!orgs.length) {
      out.className = "test-result err";
      out.textContent = "Connected, but no org privileges found.";
    } else {
      out.className = "test-result ok";
      out.textContent = `OK — ${orgs.length} org(s) found.`;
      fillOrgSelect();
      // Auto-load sites for selected org.
      await loadSitesForSelectedOrg(host, token);
    }
  } catch (e) {
    out.className = "test-result err";
    out.textContent = "Failed: " + (e?.toString() || "error");
    orgs = [];
    fillOrgSelect();
  } finally {
    btn.disabled = false;
  }
}

async function loadSitesForSelectedOrg(host, token) {
  const orgId = $("#cfg-org").value;
  if (!orgId) {
    sites = [];
    fillSiteSelect();
    return;
  }
  try {
    const list = await invoke("get_sites", { host, token, orgId });
    sites = (list || []).sort((a, b) => a.name.localeCompare(b.name));
  } catch (e) {
    console.error("get_sites", e);
    sites = [];
  }
  fillSiteSelect();
}

async function saveSettings() {
  const out = $("#save-result");
  const cluster = $("#cfg-cluster").value;
  const orgId = $("#cfg-org").value;
  const siteId = $("#cfg-site").value;

  cfg.api_host = cluster;
  cfg.custom_host = $("#cfg-custom-host").value.trim();
  cfg.token = $("#cfg-token").value.trim();
  cfg.org_id = orgId;
  cfg.org_name = orgs.find((o) => o.org_id === orgId)?.name || cfg.org_name || "";
  cfg.site_id = siteId;
  cfg.site_name = sites.find((s) => s.id === siteId)?.name || "";
  cfg.interval = Number($("#cfg-interval").value) || 60;

  if (!effectiveHost() || !cfg.token || !cfg.org_id) {
    out.className = "test-result err";
    out.textContent = "Host, token and org are required.";
    return;
  }

  await saveConfig();
  out.className = "test-result ok";
  out.textContent = "Saved.";

  applyConfigToDashboard();
  showView("dashboard");
  startPolling();
  refresh();
}

function applyConfigToDashboard() {
  $("#needs-setup").classList.toggle("hidden", isConfigured());
  $("#content").classList.toggle("hidden", !isConfigured());
  $("#btn-open-dashboard").classList.toggle("hidden", !isConfigured());
  renderScopeHeader();
  renderScopeSelect();
}

// ---------- event wiring ----------
function wireEvents() {
  $("#btn-settings").addEventListener("click", () => {
    loadSettingsForm();
    showView("settings");
  });
  $("#btn-open-settings").addEventListener("click", () => {
    loadSettingsForm();
    showView("settings");
  });
  $("#btn-back").addEventListener("click", () => {
    showView("dashboard");
  });
  $("#btn-refresh").addEventListener("click", refresh);

  for (const btn of document.querySelectorAll(".sle-tab")) {
    btn.addEventListener("click", () => selectSleTab(btn.dataset.tab));
  }

  $("#cfg-cluster").addEventListener("change", toggleCustomHost);
  $("#cfg-org").addEventListener("change", async () => {
    const host =
      $("#cfg-cluster").value === "__custom__"
        ? $("#cfg-custom-host").value.trim()
        : $("#cfg-cluster").value;
    await loadSitesForSelectedOrg(host, $("#cfg-token").value.trim());
  });
  $("#btn-test").addEventListener("click", testConnection);
  $("#btn-save").addEventListener("click", saveSettings);

  $("#scope-select").addEventListener("change", async (e) => {
    cfg.site_id = e.target.value;
    cfg.site_name = sites.find((s) => s.id === cfg.site_id)?.name || "";
    await saveConfig();
    renderScopeHeader();
    refresh();
  });

  $("#btn-open-dashboard").addEventListener("click", async () => {
    try {
      const url = await invoke("dashboard_url", {
        host: effectiveHost(),
        orgId: cfg.org_id,
        siteId: cfg.site_id || null,
      });
      await openUrl(url);
    } catch (e) {
      console.error("open dashboard", e);
    }
  });
}

// ---------- bootstrap ----------
async function init() {
  wireEvents();
  await loadConfig();

  // Preload sites for the configured org so the scope dropdown works.
  if (isConfigured()) {
    try {
      const list = await invoke("get_sites", {
        host: effectiveHost(),
        token: cfg.token,
        orgId: cfg.org_id,
      });
      sites = (list || []).sort((a, b) => a.name.localeCompare(b.name));
    } catch (e) {
      console.error("preload sites", e);
    }
    // Keep orgs list for header naming.
    orgs = [{ org_id: cfg.org_id, name: cfg.org_name }];
  }

  applyConfigToDashboard();

  if (isConfigured()) {
    showView("dashboard");
    startPolling();
    refresh();
  } else {
    showView("dashboard");
    $("#needs-setup").classList.remove("hidden");
    $("#content").classList.add("hidden");
  }

  // Refresh when the popover becomes visible again (tray re-open).
  window.addEventListener("focus", () => {
    if (isConfigured() && $("#settings").classList.contains("hidden")) refresh();
  });
}

init();
