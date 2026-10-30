// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Every device a plugin finds presents its own panel — retained, on the bus.
//!
//! PLAN-3413.01. The doctrine is that a device presents its own panel at
//! `APK.audio/System/Gui/Panels/<key>`. Until this module the only publisher
//! was the VISA hunter's `instruments/present.rs` in NODE.BareMetal, so a
//! plugin with a `gui/` folder (twelve of them) put nothing on the bus and each
//! needed a hand-built OS window. A plugin now hands the runner its template
//! once, compiled in, and the runner binds it to every device the plugin
//! publishes and keeps the panel retained while the device is there.
//!
//! WHAT A PLUGIN DECLARES. `main!(…, panels = [chromecast])` compiles in, per
//! family, the two files every `gui/` folder already holds:
//!
//! ```text
//! gui/manifest.json             { "chromecast": { "tab": …, "tree": "7_Device_Consoles" } }
//! gui/chromecast/chromecast.json  the panel, with `${model}` tokens,
//!                                 `device_state` and `device_control` markers
//! ```
//!
//! The manifest's `tree` names the placement (`7_Device_Consoles` →
//! `Device_Consoles`, the value the OS "Device Consoles" display names in its
//! `display.json` as `"panels"`). No `tree` means the Instruments display, the
//! hunter's rule.
//!
//! WHAT A DEVICE IS. A device is `…/Protocols/<token>/Device/<category>/<device>`
//! — the contract's shape (`topics::device_control` builds the same two
//! segments) — once any leaf under it has been seen. The runner subscribes to
//! `…/<token>/Device/#` and reads what the plugin already publishes there; it
//! learns nothing the bus does not already say.
//!
//! BINDING, the subset of the hunter's `panel_template` a network device needs:
//!   · `${model}`, `${family}`, `${slug}`, `${resource}`, `${name}` in any string;
//!   · `"device_state": "<field>"` → `"topic": "<device>/<field>"`;
//!   · `"device_control": "<Verb>"` → `"topic": "<device>/Control/<Verb>"`
//!     (`topics::DEVICE_CONTROL_LEAF`, the leaf the plugin's control agent reads);
//!   · `"device_doc": true` → `"topic": "<device>"`.
//! KOMAND, SCPI readouts and channel repeats are the bench's and stay there.
//!
//! WHEN A PANEL LEAVES. Unreachable is dead: a device whose `last_online` says
//! it is not online now has its panel emptied (how MQTT deletes a retained
//! message, and how the OS drops the tab). A device that falls SILENT is
//! emptied after `panels_expire_after_s` from the plugin's config.ini; with no
//! such key, silence alone never removes a panel — the tunable lives in the
//! plugin's config, not here.
//!
//! ONLY CHANGES ARE PUBLISHED. The payload text is the fingerprint, as in
//! `present.rs`, so a device restated every thirty seconds costs nothing.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use apkaudio_contracts::topics::{sanitize_segment, DEVICE_CONTROL_LEAF};
use serde_json::{json, Map, Value};

/// Where every device panel is retained. `present.rs` in NODE.BareMetal and
/// the shell's `OS.PANELS_TOPIC_ROOT` (OS/Apps/Frames/bus-panels) say the same.
pub const PANELS_TOPIC_ROOT: &str = "APK.audio/System/Gui/Panels";

/// One family's compiled-in template, parsed once.
#[derive(Clone, Debug)]
pub struct Template {
    pub family: String,
    /// The placement: the display's `"panels"` name, e.g. `Device_Consoles`.
    pub at: String,
    pub body: Value,
}

static TEMPLATES: Mutex<Vec<Template>> = Mutex::new(Vec::new());

/// Register one family. Called by [`main!`](crate::main)'s `panels = [...]`
/// through [`panel!`](crate::panel); a binary that builds its `Plugin` by hand
/// calls `apk_plugin_runner::panel!("family");` before `serve`.
///
/// A template that does not parse is reported and skipped: a broken panel must
/// not stop the hunter that finds the devices.
pub fn register(family: &str, manifest: &str, template: &str) {
    match parse_template(family, manifest, template) {
        Ok(t) => TEMPLATES.lock().unwrap_or_else(|e| e.into_inner()).push(t),
        Err(why) => eprintln!("⚠️  [panels] {family}: {why} — no panel will be presented for it"),
    }
}

/// Every family registered so far.
pub fn templates() -> Vec<Template> {
    TEMPLATES.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// `manifest.json` + `<family>.json` → a [`Template`].
pub fn parse_template(family: &str, manifest: &str, template: &str) -> Result<Template, String> {
    let manifest: Value = serde_json::from_str(manifest).map_err(|e| format!("gui/manifest.json: {e}"))?;
    let entry = manifest
        .get(family)
        .ok_or_else(|| format!("gui/manifest.json declares no `{family}`"))?;
    let at = entry
        .get("tree")
        .and_then(Value::as_str)
        .map(placement_of)
        .unwrap_or_else(|| "Instruments".to_string());
    let body: Value = serde_json::from_str(template).map_err(|e| format!("gui/{family}/{family}.json: {e}"))?;
    Ok(Template { family: family.to_string(), at, body })
}

/// `7_Device_Consoles` → `Device_Consoles`: the numeric prefix orders folders
/// on disk and is not part of the name a display matches on.
fn placement_of(tree: &str) -> String {
    match tree.split_once('_') {
        Some((n, rest)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => rest.to_string(),
        _ => tree.to_string(),
    }
}

/// One device, as the bus has described it so far.
#[derive(Debug)]
struct Device {
    category: String,
    name: String,
    fields: BTreeMap<String, String>,
    online: Option<bool>,
    seen: Instant,
}

/// The per-module state: what devices exist and which panels are on the broker.
pub struct Book {
    token: String,
    templates: Vec<Template>,
    expire_after: Option<Duration>,
    devices: BTreeMap<String, Device>,
    /// `{key: payload text}` for every panel this process has on the broker.
    ledger: BTreeMap<String, String>,
}

/// What to put on the broker, always retained: `(topic, payload)`. An empty
/// payload is a clear.
pub type Publication = (String, String);

impl Book {
    pub fn new(token: &str, templates: Vec<Template>, expire_after: Option<Duration>) -> Self {
        Book { token: token.to_string(), templates, expire_after, devices: BTreeMap::new(), ledger: BTreeMap::new() }
    }

    /// Whether this module presents anything at all.
    pub fn is_active(&self) -> bool {
        !self.templates.is_empty()
    }

    /// The device tree this module reads: `…/Protocols/<token>/Device/#`.
    pub fn filter(&self) -> String {
        format!("{}#", self.device_root())
    }

    fn device_root(&self) -> String {
        format!("APK.audio/System/Protocols/{}/Device/", self.token)
    }

    /// One message off the device tree. Returns what must be published.
    pub fn observe(&mut self, topic: &str, payload: &str, now: Instant) -> Vec<Publication> {
        let Some(rest) = topic.strip_prefix(&self.device_root()) else { return Vec::new() };
        let segments: Vec<&str> = rest.split('/').collect();
        // `<category>/<device>/<field…>`; a press on `Control/…` is the
        // operator talking to the device, not the device describing itself.
        if segments.len() < 3 || segments[2] == DEVICE_CONTROL_LEAF {
            return Vec::new();
        }
        let base = format!("{}{}/{}", self.device_root(), segments[0], segments[1]);
        let field = segments[2..].join("/");
        let device = self.devices.entry(base.clone()).or_insert_with(|| Device {
            category: segments[0].to_string(),
            name: segments[1].to_string(),
            fields: BTreeMap::new(),
            online: None,
            seen: now,
        });
        device.seen = now;
        if field == "last_online" {
            device.online = Some(apkaudio_contracts::presence::is_online_now(payload));
        } else if matches!(field.as_str(), "model" | "name" | "serial") {
            device.fields.insert(field, payload.trim().to_string());
        }
        self.reconcile(&base)
    }

    /// Clear every panel whose device has been silent past the window.
    pub fn expire(&mut self, now: Instant) -> Vec<Publication> {
        let Some(after) = self.expire_after else { return Vec::new() };
        let gone: Vec<String> = self
            .devices
            .iter()
            .filter(|(_, d)| now.saturating_duration_since(d.seen) > after)
            .map(|(base, _)| base.clone())
            .collect();
        let mut out = Vec::new();
        for base in gone {
            let device = self.devices.remove(&base).expect("listed above");
            out.extend(self.clear(&self.key_of(&device)));
        }
        out
    }

    /// Every panel on the broker, again — on a reconnect, because the broker
    /// may have lost its retained store.
    pub fn restate(&self) -> Vec<Publication> {
        self.ledger
            .iter()
            .map(|(key, text)| (format!("{PANELS_TOPIC_ROOT}/{key}"), text.clone()))
            .collect()
    }

    fn key_of(&self, device: &Device) -> String {
        sanitize_segment(&format!("{}-{}-{}", self.token, device.category, device.name))
    }

    fn clear(&mut self, key: &str) -> Vec<Publication> {
        if self.ledger.remove(key).is_some() {
            vec![(format!("{PANELS_TOPIC_ROOT}/{key}"), String::new())]
        } else {
            Vec::new()
        }
    }

    fn reconcile(&mut self, base: &str) -> Vec<Publication> {
        let Some(device) = self.devices.get(base) else { return Vec::new() };
        let key = self.key_of(device);
        if device.online == Some(false) {
            return self.clear(&key);
        }
        let document = self.document(base, device, &key);
        let Ok(text) = serde_json::to_string(&document) else { return Vec::new() };
        if self.ledger.get(&key) == Some(&text) {
            return Vec::new();
        }
        self.ledger.insert(key.clone(), text.clone());
        vec![(format!("{PANELS_TOPIC_ROOT}/{key}"), text)]
    }

    /// The `present.rs` shape: `{key, folder, device, placements:[{at, frames}]}`,
    /// one placement per registered family.
    fn document(&self, base: &str, device: &Device, key: &str) -> Value {
        let model = device.fields.get("model").cloned().unwrap_or_else(|| device.name.clone());
        let name = device.fields.get("name").cloned().unwrap_or_else(|| device.name.clone());
        let placements: Vec<Value> = self
            .templates
            .iter()
            .map(|t| {
                let mut marks = BTreeMap::new();
                marks.insert("model", model.clone());
                marks.insert("name", name.clone());
                marks.insert("family", t.family.clone());
                marks.insert("slug", key.to_string());
                marks.insert("resource", base.to_string());
                let mut content = substitute(&t.body, &marks);
                bind(&mut content, base);
                json!({
                    "at": t.at,
                    "frames": [ { "path": format!("{}.json", t.family), "content": content } ],
                })
            })
            .collect();
        json!({
            "key": key,
            // The tab's name. The device's own, with the family after it, so
            // two plugins that each found a "Living_Room" do not share a tab.
            "folder": format!("{}_{}", device.name, self.token),
            "device": {
                "type": self.templates.first().map(|t| t.family.clone()).unwrap_or_default(),
                "model": model,
                "resource": base,
                "write_topic": format!("{base}/{DEVICE_CONTROL_LEAF}"),
                "serial": device.fields.get("serial"),
            },
            "source": format!("plugin:{}", self.token),
            "placements": placements,
        })
    }
}

/// `${mark}` → its value, in every string of the document.
fn substitute(node: &Value, marks: &BTreeMap<&str, String>) -> Value {
    match node {
        Value::String(s) if s.contains("${") => {
            let mut out = s.clone();
            for (mark, value) in marks {
                out = out.replace(&format!("${{{mark}}}"), value);
            }
            Value::String(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(|v| substitute(v, marks)).collect()),
        Value::Object(map) => Value::Object(map.iter().map(|(k, v)| (k.clone(), substitute(v, marks))).collect::<Map<_, _>>()),
        other => other.clone(),
    }
}

/// Resolve the three per-device markers into topics.
fn bind(node: &mut Value, base: &str) {
    match node {
        Value::Object(map) => {
            if let Some(Value::String(field)) = map.get("device_state").cloned() {
                map.insert("topic".into(), Value::String(format!("{base}/{field}")));
            }
            if let Some(Value::String(verb)) = map.get("device_control").cloned() {
                map.insert("topic".into(), Value::String(format!("{base}/{DEVICE_CONTROL_LEAF}/{verb}")));
            }
            if map.get("device_doc") == Some(&Value::Bool(true)) {
                map.insert("topic".into(), Value::String(base.to_string()));
            }
            for (_, v) in map.iter_mut() {
                bind(v, base);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| bind(v, base)),
        _ => {}
    }
}

/// `panels_expire_after_s` from the module's own config section, if declared.
pub fn expire_after_from_config(config: &Value, token: &str) -> Option<Duration> {
    config
        .as_object()?
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(token))?
        .1
        .get("panels_expire_after_s")?
        .as_str()?
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|s| *s > 0)
        .map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{"_note": {"_": "x"}, "cast": {"tab": "7_Google_Cast", "tree": "7_Device_Consoles"}}"#;
    const TEMPLATE: &str = r#"{"Console": {"description": {"En": "Cast — ${model}"},
        "blocks": {"I": {"fields": {
            "model": {"device_state": "model"},
            "volume": {"device_control": "Volume"}}}}}}"#;

    fn book(expire: Option<u64>) -> Book {
        let t = parse_template("cast", MANIFEST, TEMPLATE).expect("parses");
        Book::new("cast", vec![t], expire.map(Duration::from_secs))
    }

    const BASE: &str = "APK.audio/System/Protocols/cast/Device/Speaker/Garage";

    #[test]
    fn the_manifest_tree_names_the_placement() {
        let t = parse_template("cast", MANIFEST, TEMPLATE).unwrap();
        assert_eq!(t.at, "Device_Consoles");
        let bare = parse_template("cast", r#"{"cast": {"tab": "x"}}"#, TEMPLATE).unwrap();
        assert_eq!(bare.at, "Instruments");
        assert!(parse_template("other", MANIFEST, TEMPLATE).is_err());
    }

    #[test]
    fn a_device_leaf_presents_a_bound_panel_once() {
        let mut b = book(None);
        let now = Instant::now();
        let out = b.observe(&format!("{BASE}/model"), "Google Nest Mini", now);
        assert_eq!(out.len(), 1);
        let (topic, text) = &out[0];
        assert_eq!(topic, "APK.audio/System/Gui/Panels/cast-Speaker-Garage");
        let doc: Value = serde_json::from_str(text).unwrap();
        assert_eq!(doc["placements"][0]["at"], "Device_Consoles");
        let content = &doc["placements"][0]["frames"][0]["content"]["Console"];
        assert_eq!(content["description"]["En"], "Cast — Google Nest Mini");
        assert_eq!(content["blocks"]["I"]["fields"]["model"]["topic"], format!("{BASE}/model"));
        assert_eq!(content["blocks"]["I"]["fields"]["volume"]["topic"], format!("{BASE}/Control/Volume"));
        // The same fact again is not a publish.
        assert!(b.observe(&format!("{BASE}/model"), "Google Nest Mini", now).is_empty());
        // A press on the control topic is not the device describing itself.
        assert!(b.observe(&format!("{BASE}/Control/Volume"), "0.5", now).is_empty());
    }

    #[test]
    fn a_device_that_is_not_online_has_its_panel_emptied() {
        let mut b = book(None);
        let now = Instant::now();
        assert_eq!(b.observe(&format!("{BASE}/name"), "Garage", now).len(), 1);
        let out = b.observe(&format!("{BASE}/last_online"), "1761830000", now);
        assert_eq!(out, vec![("APK.audio/System/Gui/Panels/cast-Speaker-Garage".to_string(), String::new())]);
        assert!(b.restate().is_empty());
    }

    #[test]
    fn silence_expires_only_when_the_config_says_so() {
        let now = Instant::now();
        let later = now + Duration::from_secs(600);
        let mut never = book(None);
        never.observe(&format!("{BASE}/name"), "Garage", now);
        assert!(never.expire(later).is_empty());

        let mut b = book(Some(120));
        b.observe(&format!("{BASE}/name"), "Garage", now);
        assert!(b.expire(now + Duration::from_secs(60)).is_empty());
        let out = b.expire(later);
        assert_eq!(out.len(), 1);
        assert!(out[0].1.is_empty());
    }

    #[test]
    fn the_expiry_is_read_from_the_modules_own_section() {
        let config = json!({"chromecast": {"panels_expire_after_s": "120"}});
        assert_eq!(expire_after_from_config(&config, "chromecast"), Some(Duration::from_secs(120)));
        assert_eq!(expire_after_from_config(&json!({"chromecast": {}}), "chromecast"), None);
    }
}
