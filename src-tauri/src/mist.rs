use serde::Serialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::task::JoinSet;

/// A single SLE metric result. `value` is a 0..100 success-rate percentage,
/// or None when the metric is unavailable (404 / no samples / license gap).
#[derive(Serialize, Clone)]
pub struct SleMetric {
    pub name: String,  // display name, e.g. "Coverage"
    pub key: String,   // primary API key, e.g. "coverage"
    pub value: Option<f64>,
}

/// All SLE metrics for a scope, grouped by category.
#[derive(Serialize, Clone)]
pub struct SiteSleData {
    pub wireless: Vec<SleMetric>,
    pub wired: Vec<SleMetric>,
    pub wan: Vec<SleMetric>,
}

/// Definition of one SLE metric: which category it belongs to, its display
/// name, and the API key(s) to try (first is primary, rest are fallbacks for
/// the singular/plural / renamed variants Mist uses across versions).
struct MetricDef {
    cat: &'static str,
    name: &'static str,
    keys: &'static [&'static str],
}

/// The metrics fetched for a site, in display order per category.
const SITE_METRICS: &[MetricDef] = &[
    // ---- Wireless ----
    MetricDef { cat: "wireless", name: "Coverage", keys: &["coverage"] },
    MetricDef { cat: "wireless", name: "Roaming", keys: &["roaming"] },
    MetricDef { cat: "wireless", name: "Time to Connect", keys: &["time-to-connect"] },
    // `failed-to-connect` is the currently-enabled metric; its `degraded`
    // samples are failures, so (1 - degraded/total)*100 yields success rate.
    MetricDef { cat: "wireless", name: "Successful Connects", keys: &["successful-connects", "successful-connect", "failed-to-connect"] },
    MetricDef { cat: "wireless", name: "Capacity", keys: &["capacity"] },
    MetricDef { cat: "wireless", name: "AP Health", keys: &["ap-health"] },
    MetricDef { cat: "wireless", name: "Throughput", keys: &["throughput"] },
    // ---- Wired ----
    MetricDef { cat: "wired", name: "Switch Health", keys: &["switch-health"] },
    MetricDef { cat: "wired", name: "Switch Throughput", keys: &["switch-throughput", "wired-throughput"] },
    MetricDef { cat: "wired", name: "Successful Connect", keys: &["successful-connect", "wired-successful-connects", "successful-connects"] },
    // ---- WAN ----
    MetricDef { cat: "wan", name: "WAN Link Health", keys: &["wan-link-health"] },
    MetricDef { cat: "wan", name: "WAN Edge Health", keys: &["wan-edge-health", "gateway-health"] },
    MetricDef { cat: "wan", name: "Application Health", keys: &["application-health", "wan-application-health"] },
];

/// A thin Mist API client. All calls are GET, 10s timeout, Token auth.
#[derive(Clone)]
pub struct MistClient {
    http: reqwest::Client,
    base: String,
    token: String,
}

#[derive(Serialize)]
pub struct OrgInfo {
    pub org_id: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct SiteInfo {
    pub id: String,
    pub name: String,
}

impl MistClient {
    pub fn new(host: &str, token: &str) -> Result<Self, String> {
        let host = host.trim().trim_end_matches('/');
        if host.is_empty() {
            return Err("API host is empty".into());
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            http,
            base: format!("https://{host}/api/v1"),
            token: token.trim().to_string(),
        })
    }

    /// GET a path under /api/v1 and parse JSON. Returns Err on network / status / parse errors.
    async fn get(&self, path: &str) -> Result<Value, String> {
        let url = format!("{}{}", self.base, path);
        let resp = self
            .http
            .get(&url)
            .header("Authorization", format!("Token {}", self.token))
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("request error: {e}"))?;

        let status = resp.status();
        if status.as_u16() == 429 {
            return Err("rate limited (429)".into());
        }
        if !status.is_success() {
            return Err(format!("HTTP {}", status.as_u16()));
        }
        resp.json::<Value>()
            .await
            .map_err(|e| format!("parse error: {e}"))
    }

    /// Soft GET: errors become None so a failing sub-call degrades to N/A.
    async fn get_opt(&self, path: &str) -> Option<Value> {
        match self.get(path).await {
            Ok(v) => Some(v),
            Err(e) => {
                eprintln!("[mist] GET {path} failed: {e}");
                None
            }
        }
    }

    // ---------------- self / orgs / sites ----------------

    pub async fn get_self(&self) -> Result<Vec<OrgInfo>, String> {
        let v = self.get("/self").await?;
        let mut orgs: Vec<OrgInfo> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        if let Some(privs) = v.get("privileges").and_then(|p| p.as_array()) {
            for p in privs {
                let scope = p.get("scope").and_then(|s| s.as_str()).unwrap_or("");
                if scope != "org" {
                    continue;
                }
                if let Some(org_id) = p.get("org_id").and_then(|s| s.as_str()) {
                    if seen.insert(org_id.to_string()) {
                        let name = p
                            .get("name")
                            .and_then(|s| s.as_str())
                            .unwrap_or(org_id)
                            .to_string();
                        orgs.push(OrgInfo {
                            org_id: org_id.to_string(),
                            name,
                        });
                    }
                }
            }
        }
        Ok(orgs)
    }

    pub async fn get_sites(&self, org_id: &str) -> Result<Vec<SiteInfo>, String> {
        let v = self.get(&format!("/orgs/{org_id}/sites")).await?;
        let mut out = Vec::new();
        if let Some(arr) = v.as_array() {
            for s in arr {
                if let Some(id) = s.get("id").and_then(|x| x.as_str()) {
                    let name = s
                        .get("name")
                        .and_then(|x| x.as_str())
                        .unwrap_or(id)
                        .to_string();
                    out.push(SiteInfo {
                        id: id.to_string(),
                        name,
                    });
                }
            }
        }
        Ok(out)
    }

    // ---------------- dashboard aggregation ----------------

    pub async fn get_dashboard(&self, org_id: &str, site_id: Option<&str>) -> Value {
        match site_id {
            Some(sid) if !sid.is_empty() => {
                let (sle, devices, clients, alarms) = tokio::join!(
                    self.site_sle(sid),
                    self.devices(Some(sid), org_id),
                    self.site_clients(sid),
                    self.alarms(Some(sid), org_id),
                );
                json!({ "sle": sle, "devices": devices, "clients": clients, "alarms": alarms })
            }
            _ => {
                let (sle, devices, clients, alarms) = tokio::join!(
                    self.org_sle(org_id),
                    self.devices(None, org_id),
                    self.org_clients(org_id),
                    self.alarms(None, org_id),
                );
                json!({ "sle": sle, "devices": devices, "clients": clients, "alarms": alarms })
            }
        }
    }

    // ---- SLE: org ----
    // Org Insights has no per-metric breakdown, so we surface a single
    // "Health" entry per category (extracted from /insights/stats) using the
    // same structured shape the site path produces.
    async fn org_sle(&self, org_id: &str) -> SiteSleData {
        let v = self.get_opt(&format!("/orgs/{org_id}/insights/stats")).await;
        let v = match v {
            Some(v) => v,
            None => return SiteSleData { wireless: vec![], wired: vec![], wan: vec![] },
        };
        eprintln!("[mist] org insights keys: {:?}", top_keys(&v));
        let one = |kws: &[&str]| -> Vec<SleMetric> {
            vec![SleMetric {
                name: "Health".into(),
                key: "health".into(),
                value: find_health(&v, kws),
            }]
        };
        SiteSleData {
            wireless: one(&["wireless", "wifi"]),
            wired: one(&["wired", "switch"]),
            wan: one(&["wan", "gateway"]),
        }
    }

    // ---- SLE: site (per-metric summaries) ----
    // Fetches every metric in SITE_METRICS in parallel via the per-metric
    // summary endpoint and returns each one individually (no averaging).
    async fn site_sle(&self, site_id: &str) -> SiteSleData {
        let mut set: JoinSet<(usize, Option<f64>)> = JoinSet::new();
        for (i, m) in SITE_METRICS.iter().enumerate() {
            let client = self.clone();
            let site = site_id.to_string();
            let keys = m.keys;
            set.spawn(async move { (i, client.fetch_metric(&site, keys).await) });
        }

        let mut values: Vec<Option<f64>> = vec![None; SITE_METRICS.len()];
        while let Some(res) = set.join_next().await {
            if let Ok((i, v)) = res {
                values[i] = v;
            }
        }

        let mut data = SiteSleData { wireless: vec![], wired: vec![], wan: vec![] };
        for (i, m) in SITE_METRICS.iter().enumerate() {
            let metric = SleMetric {
                name: m.name.into(),
                key: m.keys[0].into(),
                value: values[i],
            };
            match m.cat {
                "wireless" => data.wireless.push(metric),
                "wired" => data.wired.push(metric),
                "wan" => data.wan.push(metric),
                _ => {}
            }
        }
        data
    }

    /// Fetch one metric's summary, trying alternate key names on failure.
    /// Endpoint: GET /sites/{id}/sle/site/{id}/metric/{metric}/summary?duration=1d
    async fn fetch_metric(&self, site_id: &str, keys: &[&str]) -> Option<f64> {
        for k in keys {
            let path = format!(
                "/sites/{site_id}/sle/site/{site_id}/metric/{k}/summary?duration=1d"
            );
            match self.get(&path).await {
                Ok(v) => {
                    if let Some(rate) = extract_sle_rate(&v) {
                        return Some(rate);
                    }
                    // 200 but no usable samples: a valid alternate key won't help
                    // unless this was the wrong name — keep trying alternates.
                }
                Err(e) => {
                    eprintln!("[mist] metric {k} failed: {e}");
                }
            }
        }
        None
    }

    // ---- Devices ----
    async fn devices(&self, site_id: Option<&str>, org_id: &str) -> Value {
        let base = match site_id {
            Some(sid) => format!("/sites/{sid}/stats/devices"),
            None => format!("/orgs/{org_id}/stats/devices"),
        };

        // Try type=all first.
        let mut all: Vec<Value> = Vec::new();
        if let Some(v) = self.get_opt(&format!("{base}?type=all&limit=1000")).await {
            if let Some(arr) = v.as_array() {
                all = arr.clone();
            }
        }
        // Fallback: fetch per type if type=all returned nothing.
        if all.is_empty() {
            for t in ["ap", "switch", "gateway"] {
                if let Some(v) = self.get_opt(&format!("{base}?type={t}&limit=1000")).await {
                    if let Some(arr) = v.as_array() {
                        all.extend(arr.iter().cloned());
                    }
                }
            }
        }

        if all.is_empty() {
            return json!({ "ap": null, "switch": null, "gateway": null });
        }

        let mut counts: std::collections::HashMap<&str, (u32, u32)> = std::collections::HashMap::new();
        for d in &all {
            let typ = d.get("type").and_then(|x| x.as_str()).unwrap_or("");
            let key = match typ {
                "ap" => "ap",
                "switch" => "switch",
                "gateway" => "gateway",
                _ => continue,
            };
            let status = d.get("status").and_then(|x| x.as_str()).unwrap_or("");
            let connected = status == "connected";
            let entry = counts.entry(key).or_insert((0, 0));
            entry.0 += 1; // total
            if connected {
                entry.1 += 1; // connected
            }
        }

        let mk = |k: &str| -> Value {
            match counts.get(k) {
                Some((total, conn)) => json!({
                    "total": total,
                    "connected": conn,
                    "down": total - conn,
                }),
                None => Value::Null,
            }
        };
        json!({ "ap": mk("ap"), "switch": mk("switch"), "gateway": mk("gateway") })
    }

    // ---- Clients: org ----
    async fn org_clients(&self, org_id: &str) -> Value {
        let wpath = format!("/orgs/{org_id}/stats/clients/search?duration=1h&limit=1");
        let epath = format!("/orgs/{org_id}/wired_clients/search?duration=1h&limit=1");
        let (wireless, wired) = tokio::join!(self.get_opt(&wpath), self.get_opt(&epath));
        json!({
            "wireless": wireless.as_ref().and_then(total_field),
            "wired": wired.as_ref().and_then(total_field),
        })
    }

    // ---- Clients: site ----
    async fn site_clients(&self, site_id: &str) -> Value {
        let wpath = format!("/sites/{site_id}/stats/clients");
        let epath = format!("/sites/{site_id}/stats/wired_clients");
        let (wireless, wired) = tokio::join!(self.get_opt(&wpath), self.get_opt(&epath));
        let wcount = wireless.as_ref().and_then(|v| v.as_array()).map(|a| a.len() as u64);
        // Wired stats endpoint may differ; fall back to /wired_clients.
        let mut ecount = wired.as_ref().and_then(|v| v.as_array()).map(|a| a.len() as u64);
        if ecount.is_none() {
            if let Some(v) = self.get_opt(&format!("/sites/{site_id}/wired_clients")).await {
                ecount = v.as_array().map(|a| a.len() as u64);
            }
        }
        json!({ "wireless": wcount, "wired": ecount })
    }

    // ---- Alarms ----
    async fn alarms(&self, site_id: Option<&str>, org_id: &str) -> Value {
        let path = match site_id {
            Some(sid) => format!("/sites/{sid}/alarms/search?duration=1d&limit=100"),
            None => format!("/orgs/{org_id}/alarms/search?duration=1d&limit=100"),
        };
        let v = match self.get_opt(&path).await {
            Some(v) => v,
            None => return Value::Null,
        };
        let results = v.get("results").and_then(|r| r.as_array());
        let total = v
            .get("total")
            .and_then(|t| t.as_u64())
            .or_else(|| results.map(|r| r.len() as u64))
            .unwrap_or(0);

        let mut sev: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        if let Some(arr) = results {
            for a in arr {
                let s = a
                    .get("severity")
                    .and_then(|x| x.as_str())
                    .unwrap_or("info")
                    .to_lowercase();
                *sev.entry(s).or_insert(0) += 1;
            }
        }
        json!({ "total": total, "severities": sev })
    }

    pub fn dashboard_url(host: &str, org_id: &str, site_id: Option<&str>) -> String {
        let manage = host.replacen("api.", "manage.", 1);
        match site_id {
            Some(sid) if !sid.is_empty() => format!(
                "https://{manage}/admin/?org_id={org_id}#!dashboard/insights/site/{sid}"
            ),
            _ => format!("https://{manage}/admin/?org_id={org_id}#!dashboard/insights/org"),
        }
    }
}

fn total_field(v: &Value) -> Option<u64> {
    v.get("total").and_then(|t| t.as_u64())
}

fn top_keys(v: &Value) -> Vec<String> {
    match v {
        Value::Object(m) => m.keys().cloned().collect(),
        Value::Array(_) => vec!["<array>".into()],
        _ => vec![],
    }
}

/// Normalize a raw numeric SLE/health value into a 0..100 percentage.
fn normalize_pct(n: f64) -> f64 {
    if n <= 1.0 && n >= 0.0 {
        n * 100.0
    } else {
        n
    }
}

/// Recursively search a JSON tree for a numeric value whose key-path mentions
/// one of `keywords` AND a health-like term. Returns the first match, normalized.
fn find_health(v: &Value, keywords: &[&str]) -> Option<f64> {
    fn walk(v: &Value, path: &str, keywords: &[&str]) -> Option<f64> {
        match v {
            Value::Object(map) => {
                for (k, val) in map {
                    let kl = k.to_lowercase();
                    let p = if path.is_empty() {
                        kl.clone()
                    } else {
                        format!("{path}.{kl}")
                    };
                    if let Some(n) = val.as_f64() {
                        let matches_kw = keywords.iter().any(|kw| p.contains(kw));
                        let matches_health = p.contains("health")
                            || p.contains("sle")
                            || p.contains("score")
                            || p.contains("success");
                        if matches_kw && matches_health && (0.0..=100.0).contains(&n) {
                            return Some(normalize_pct(n));
                        }
                    } else if let Some(found) = walk(val, &p, keywords) {
                        return Some(found);
                    }
                }
                None
            }
            Value::Array(arr) => {
                for item in arr {
                    if let Some(found) = walk(item, path, keywords) {
                        return Some(found);
                    }
                }
                None
            }
            _ => None,
        }
    }
    walk(v, "", keywords)
}

/// Extract a success rate (0–100 %) from a per-metric SLE summary response.
///
/// MUST read from sle.samples directly.
/// Recursive search (find_degraded_total) incorrectly picks up
/// classifiers[].samples first, where degraded≈0 → rate≈100%.
fn extract_sle_rate(v: &Value) -> Option<f64> {
    let samples = v.get("sle")?.get("samples")?;

    // degraded and total are time-series arrays of f64 (null entries skipped)
    let sum_degraded: f64 = samples
        .get("degraded")?
        .as_array()?
        .iter()
        .filter_map(|x| x.as_f64())
        .sum();

    let sum_total: f64 = samples
        .get("total")?
        .as_array()?
        .iter()
        .filter_map(|x| x.as_f64())
        .sum();

    if sum_total == 0.0 {
        return None;
    }

    Some(((1.0 - sum_degraded / sum_total) * 100.0).clamp(0.0, 100.0))
}

/// Extract a success rate (%) from an SLE summary response. Tries several shapes:
///  - direct health/sle/score percentage
///  - total + ok/good/success  => ok/total
///  - total + degraded/bad      => (total-degraded)/total
fn extract_rate(v: &Value) -> Option<f64> {
    // 1) direct percentage anywhere.
    if let Some(p) = find_any_pct(v) {
        return Some(p);
    }
    // 2) aggregate counters.
    let total = sum_keys(v, &["total"]);
    let ok = sum_keys(v, &["ok", "good", "success", "successful"]);
    let degraded = sum_keys(v, &["degraded", "bad", "failed", "fail"]);
    if total > 0.0 {
        if ok > 0.0 {
            return Some((ok / total * 100.0).clamp(0.0, 100.0));
        }
        if degraded >= 0.0 && (ok > 0.0 || degraded > 0.0) {
            return Some(((total - degraded) / total * 100.0).clamp(0.0, 100.0));
        }
    }
    None
}

fn find_any_pct(v: &Value) -> Option<f64> {
    fn walk(v: &Value, path: &str) -> Option<f64> {
        match v {
            Value::Object(map) => {
                for (k, val) in map {
                    let kl = k.to_lowercase();
                    let p = if path.is_empty() {
                        kl.clone()
                    } else {
                        format!("{path}.{kl}")
                    };
                    if let Some(n) = val.as_f64() {
                        if (p.contains("health") || p.contains("sle") || p.contains("score"))
                            && (0.0..=100.0).contains(&normalize_pct(n))
                        {
                            return Some(normalize_pct(n));
                        }
                    } else if let Some(found) = walk(val, &p) {
                        return Some(found);
                    }
                }
                None
            }
            Value::Array(arr) => arr.iter().find_map(|i| walk(i, path)),
            _ => None,
        }
    }
    walk(v, "")
}

/// Sum all numeric leaves whose key exactly matches one of `keys` (case-insensitive).
fn sum_keys(v: &Value, keys: &[&str]) -> f64 {
    let mut acc = 0.0;
    fn walk(v: &Value, keys: &[&str], acc: &mut f64) {
        match v {
            Value::Object(map) => {
                for (k, val) in map {
                    let kl = k.to_lowercase();
                    if keys.contains(&kl.as_str()) {
                        if let Some(n) = val.as_f64() {
                            *acc += n;
                        }
                    }
                    walk(val, keys, acc);
                }
            }
            Value::Array(arr) => {
                for i in arr {
                    walk(i, keys, acc);
                }
            }
            _ => {}
        }
    }
    walk(v, keys, &mut acc);
    acc
}
