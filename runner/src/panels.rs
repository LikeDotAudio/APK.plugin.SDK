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
//! A manifest entry may also say `"device_depth": 1` for a plugin whose
//! devices sit one level under `Device/` (`…/Device/<device>/<field>`, the
//! shape the PlayStation, NMOS and Dante hunters write) instead of the
//! contract's two (`<category>/<device>`).
//!
//! `"device_path"` names the shape outright, as segments under `Device/`
//! where `+` is any one segment and anything else must match itself:
//! `"+/Channel/+"` makes every `…/Device/<device>/Channel/<n>` its own device
//! (Dante's per-channel console). `device_depth: 1` is `"+"`, the default `"+/+"`.
//! A family with its own shape is its own panel; families of one shape share
//! one panel, a placement each.
//!
//! `"device_document": true` says the device is the topic AT that path and its
//! payload is one JSON object (SAP's `…/Device/<origin>`): its top-level
//! `last_online`, `model`, `name` and `serial` are read as the leaves would be,
//! and a `"device_doc": true` field binds to the topic itself.
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
//! A PANEL AN EARLIER PROCESS LEFT. The book also reads back
//! `Gui/Panels/+` and adopts every retained panel whose `source` is this
//! plugin's but which this process has not presented. If no device accounts
//! for it within [`ADOPT_GRACE`](crate::presence::ADOPT_GRACE) — the window
//! the presence tracker gives a retained device nobody has sighted — it is
//! emptied, so a device that left while the plugin was down does not keep a
//! tab for ever (LOOSE-panels-stale-after-restart).
//!
//! ONLY CHANGES ARE PUBLISHED. The payload text is the fingerprint, as in
//! `present.rs`, so a device restated every thirty seconds costs nothing.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::presence::ADOPT_GRACE;
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
    /// The segments under `Device/` that name one device: `+` is any one,
    /// anything else must match itself. `["+", "+"]` (`<category>/<device>`,
    /// the contract's shape) unless the manifest says otherwise.
    pub path: Vec<String>,
    /// The device is the topic at `path` and its payload one JSON object.
    pub document: bool,
    pub body: Value,
}

/// Any one segment, in a [`Template::path`].
const ANY: &str = "+";

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
    let depth = match entry.get("device_depth") {
        None => 2,
        Some(v) => match v.as_u64() {
            Some(1) => 1,
            Some(2) => 2,
            _ => return Err(format!("gui/manifest.json `{family}.device_depth` is {v}; it is 1 or 2")),
        },
    };
    let path: Vec<String> = match entry.get("device_path") {
        None => vec![ANY.to_string(); depth],
        Some(v) => {
            let text = v.as_str().ok_or_else(|| format!("gui/manifest.json `{family}.device_path` is {v}; it is a string"))?;
            let path: Vec<String> = text.split('/').map(str::to_string).collect();
            if path.iter().any(|s| s.is_empty() || s == "#" || s == DEVICE_CONTROL_LEAF) || !path.iter().any(|s| s == ANY) {
                return Err(format!("gui/manifest.json `{family}.device_path` is {text:?}; it is segments with at least one `+`"));
            }
            path
        }
    };
    let document = match entry.get("device_document") {
        None => false,
        Some(v) => v.as_bool().ok_or_else(|| format!("gui/manifest.json `{family}.device_document` is {v}; it is true or false"))?,
    };
    let body: Value = serde_json::from_str(template).map_err(|e| format!("gui/{family}/{family}.json: {e}"))?;
    Ok(Template { family: family.to_string(), at, path, document, body })
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
    /// Which [`Shape`] found it.
    shape: usize,
    /// Its segments under `Device/`, literals included.
    segments: Vec<String>,
    name: String,
    fields: BTreeMap<String, String>,
    online: Option<bool>,
    seen: Instant,
}

/// The families that read one device shape, presented as one panel.
struct Shape {
    path: Vec<String>,
    document: bool,
    templates: Vec<Template>,
}

impl Shape {
    /// `segments` (under `Device/`) → `(device segments, field)` when they
    /// name a leaf of a device of this shape (or, for a document shape, the
    /// device itself, with an empty field).
    fn find<'a>(&self, segments: &'a [&'a str]) -> Option<(&'a [&'a str], String)> {
        let n = self.path.len();
        if segments.len() < n || segments[..n].iter().zip(&self.path).any(|(s, p)| s.is_empty() || (p != ANY && p != s)) {
            return None;
        }
        if self.document {
            return (segments.len() == n).then(|| (&segments[..n], String::new()));
        }
        // A press on `Control/…` is the operator talking to the device, not
        // the device describing itself.
        if segments.len() == n || segments[n] == DEVICE_CONTROL_LEAF {
            return None;
        }
        Some((&segments[..n], segments[n..].join("/")))
    }

    /// The tab's name: the device's own segment for the contract's
    /// `<category>/<device>` and a flat `<device>`, every `+` segment otherwise.
    fn name_of(&self, segments: &[&str]) -> String {
        if self.path.len() <= 2 {
            return segments.last().map(|s| s.to_string()).unwrap_or_default();
        }
        segments.iter().zip(&self.path).filter(|(_, p)| *p == ANY).map(|(s, _)| *s).collect::<Vec<_>>().join("_")
    }
}

/// The per-module state: what devices exist and which panels are on the broker.
pub struct Book {
    token: String,
    shapes: Vec<Shape>,
    expire_after: Option<Duration>,
    devices: BTreeMap<String, Device>,
    /// `{key: payload text}` for every panel this process has on the broker.
    ledger: BTreeMap<String, String>,
    /// `{key: when first read back}` for every retained panel of this plugin's
    /// that an earlier process left and no device here has claimed yet.
    inherited: BTreeMap<String, Instant>,
}

/// What to put on the broker, always retained: `(topic, payload)`. An empty
/// payload is a clear.
pub type Publication = (String, String);

impl Book {
    pub fn new(token: &str, templates: Vec<Template>, expire_after: Option<Duration>) -> Self {
        let mut shapes: Vec<Shape> = Vec::new();
        for t in templates {
            match shapes.iter_mut().find(|s| s.path == t.path && s.document == t.document) {
                Some(shape) => shape.templates.push(t),
                None => shapes.push(Shape { path: t.path.clone(), document: t.document, templates: vec![t] }),
            }
        }
        Book { token: token.to_string(), shapes, expire_after, devices: BTreeMap::new(), ledger: BTreeMap::new(), inherited: BTreeMap::new() }
    }

    /// Whether this module presents anything at all.
    pub fn is_active(&self) -> bool {
        !self.shapes.is_empty()
    }

    /// The device tree this module reads: `…/Protocols/<token>/Device/#`.
    pub fn filter(&self) -> String {
        format!("{}#", self.device_root())
    }

    /// The panels this module may have left on the broker: `Gui/Panels/+`,
    /// read back so a panel no device claims can be emptied.
    pub fn panels_filter(&self) -> String {
        format!("{PANELS_TOPIC_ROOT}/+")
    }

    fn device_root(&self) -> String {
        format!("APK.audio/System/Protocols/{}/Device/", self.token)
    }

    /// One message off the device tree. Returns what must be published.
    pub fn observe(&mut self, topic: &str, payload: &str, now: Instant) -> Vec<Publication> {
        if let Some(key) = topic.strip_prefix(PANELS_TOPIC_ROOT).and_then(|k| k.strip_prefix('/')) {
            self.read_back(key, payload, now);
            return Vec::new();
        }
        let Some(rest) = topic.strip_prefix(&self.device_root()) else { return Vec::new() };
        let segments: Vec<&str> = rest.split('/').collect();
        let root = self.device_root();
        let found: Vec<(usize, bool, &[&str], String, String)> = self
            .shapes
            .iter()
            .enumerate()
            .filter_map(|(index, shape)| shape.find(&segments).map(|(at, field)| (index, shape.document, at, field, shape.name_of(at))))
            .collect();
        let mut out = Vec::new();
        for (index, document, at, field, name) in found {
            let base = format!("{root}{}", at.join("/"));
            let device = self.devices.entry(base.clone()).or_insert_with(|| Device {
                shape: index,
                segments: at.iter().map(|s| s.to_string()).collect(),
                name,
                fields: BTreeMap::new(),
                online: None,
                seen: now,
            });
            device.seen = now;
            if document {
                // The document is the whole device. An empty one is a device
                // that said it is gone, the way a cleared leaf would.
                match serde_json::from_str::<Value>(payload) {
                    Ok(Value::Object(doc)) => {
                        let text = |v: &Value| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string());
                        if let Some(v) = doc.get("last_online") {
                            device.online = Some(apkaudio_contracts::presence::is_online_now(&text(v)));
                        }
                        for leaf in ["model", "name", "serial"] {
                            if let Some(v) = doc.get(leaf) {
                                device.fields.insert(leaf.to_string(), text(v).trim().to_string());
                            }
                        }
                    }
                    _ if payload.trim().is_empty() => device.online = Some(false),
                    _ => {}
                }
            } else if field == "last_online" {
                device.online = Some(apkaudio_contracts::presence::is_online_now(payload));
            } else if matches!(field.as_str(), "model" | "name" | "serial") {
                device.fields.insert(field, payload.trim().to_string());
            }
            out.extend(self.reconcile(&base));
        }
        out
    }

    /// A retained panel on the broker. One of this plugin's that this process
    /// did not present is an earlier process's, held until a device claims it
    /// or [`ADOPT_GRACE`] passes. An empty payload is a clear, ours or not.
    fn read_back(&mut self, key: &str, payload: &str, now: Instant) {
        if payload.trim().is_empty() {
            self.inherited.remove(key);
            return;
        }
        if self.ledger.contains_key(key) {
            return;
        }
        let source = serde_json::from_str::<Value>(payload)
            .ok()
            .and_then(|d| d.get("source").and_then(Value::as_str).map(str::to_string));
        if source.as_deref() == Some(format!("plugin:{}", self.token).as_str()) {
            self.inherited.entry(key.to_string()).or_insert(now);
        }
    }

    /// Clear every panel whose device has been silent past the window, and
    /// every inherited panel no device has claimed within [`ADOPT_GRACE`].
    pub fn expire(&mut self, now: Instant) -> Vec<Publication> {
        let orphans: Vec<String> = self
            .inherited
            .iter()
            .filter(|(_, since)| now.saturating_duration_since(**since) > ADOPT_GRACE)
            .map(|(key, _)| key.clone())
            .collect();
        let mut out = Vec::new();
        for key in orphans {
            out.extend(self.clear(&key));
        }
        let Some(after) = self.expire_after else { return out };
        let gone: Vec<String> = self
            .devices
            .iter()
            .filter(|(_, d)| now.saturating_duration_since(d.seen) > after)
            .map(|(base, _)| base.clone())
            .collect();
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

    /// `<token>-<segment>-…`: `cast-Speaker-Garage`, `playstation-B41F…`,
    /// `dante-<device>-Channel-3`.
    fn key_of(&self, device: &Device) -> String {
        sanitize_segment(&format!("{}-{}", self.token, device.segments.join("-")))
    }

    fn clear(&mut self, key: &str) -> Vec<Publication> {
        let ours = self.ledger.remove(key).is_some();
        let inherited = self.inherited.remove(key).is_some();
        if ours || inherited {
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
        self.inherited.remove(&key);
        self.ledger.insert(key.clone(), text.clone());
        vec![(format!("{PANELS_TOPIC_ROOT}/{key}"), text)]
    }

    /// The `present.rs` shape: `{key, folder, device, placements:[{at, frames}]}`,
    /// one placement per registered family.
    fn document(&self, base: &str, device: &Device, key: &str) -> Value {
        let model = device.fields.get("model").cloned().unwrap_or_else(|| device.name.clone());
        let name = device.fields.get("name").cloned().unwrap_or_else(|| device.name.clone());
        let templates = &self.shapes[device.shape].templates;
        let placements: Vec<Value> = templates
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
                "type": templates.first().map(|t| t.family.clone()).unwrap_or_default(),
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

    fn left_behind(key: &str, token: &str) -> (String, String) {
        (format!("{PANELS_TOPIC_ROOT}/{key}"), json!({"key": key, "source": format!("plugin:{token}")}).to_string())
    }

    #[test]
    fn a_panel_an_earlier_process_left_is_emptied_when_no_device_claims_it() {
        let mut b = book(None);
        let now = Instant::now();
        let (topic, text) = left_behind("cast-Speaker-Gone", "cast");
        assert!(b.observe(&topic, &text, now).is_empty());
        // Another plugin's panel is not ours to clear.
        let (other, other_text) = left_behind("dnssd-Host-X", "dnssd");
        b.observe(&other, &other_text, now);
        assert!(b.expire(now + ADOPT_GRACE).is_empty(), "not before the grace");
        let out = b.expire(now + ADOPT_GRACE + Duration::from_secs(1));
        assert_eq!(out, vec![(topic, String::new())]);
        assert!(b.expire(now + ADOPT_GRACE * 10).is_empty(), "said once");
    }

    #[test]
    fn an_inherited_panel_a_device_claims_is_kept() {
        let mut b = book(None);
        let now = Instant::now();
        let (topic, text) = left_behind("cast-Speaker-Garage", "cast");
        b.observe(&topic, &text, now);
        assert_eq!(b.observe(&format!("{BASE}/name"), "Garage", now).len(), 1);
        // Our own echo of what we just presented is not an inheritance.
        let (_, ours) = b.restate().remove(0);
        b.observe(&topic, &ours, now);
        assert!(b.expire(now + ADOPT_GRACE * 2).is_empty());
    }

    #[test]
    fn an_inherited_panel_whose_device_reads_offline_is_emptied_at_once() {
        let mut b = book(None);
        let now = Instant::now();
        let (topic, text) = left_behind("cast-Speaker-Garage", "cast");
        b.observe(&topic, &text, now);
        let out = b.observe(&format!("{BASE}/last_online"), "1761830000", now);
        assert_eq!(out, vec![(topic.clone(), String::new())]);
        // The broker's echo of that clear ends it.
        b.observe(&topic, "", now);
        assert!(b.expire(now + ADOPT_GRACE * 2).is_empty());
    }

    #[test]
    fn a_flat_device_tree_is_read_at_depth_one() {
        let manifest = r#"{"ps": {"tree": "7_Device_Consoles", "device_depth": 1}}"#;
        let t = parse_template("ps", manifest, TEMPLATE).expect("parses");
        assert_eq!(t.path, vec!["+"]);
        let mut b = Book::new("playstation", vec![t], None);
        let now = Instant::now();
        let base = "APK.audio/System/Protocols/playstation/Device/B41F4D477C99";
        let out = b.observe(&format!("{base}/model"), "PS5", now);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "APK.audio/System/Gui/Panels/playstation-B41F4D477C99");
        let doc: Value = serde_json::from_str(&out[0].1).unwrap();
        let content = &doc["placements"][0]["frames"][0]["content"]["Console"];
        assert_eq!(content["blocks"]["I"]["fields"]["model"]["topic"], format!("{base}/model"));
        assert_eq!(content["blocks"]["I"]["fields"]["volume"]["topic"], format!("{base}/Control/Volume"));
        // A deeper leaf belongs to the same device, a press is not a fact.
        assert!(b.observe(&format!("{base}/Channel/Ch1/id"), "1", now).is_empty());
        assert!(b.observe(&format!("{base}/Control/Wake"), "1", now).is_empty());
        assert!(parse_template("ps", r#"{"ps": {"device_depth": 3}}"#, TEMPLATE).is_err());
    }

    #[test]
    fn a_device_path_makes_each_channel_its_own_panel_beside_the_device() {
        let manifest = r#"{"dante": {"tree": "7_Device_Consoles", "device_depth": 1},
                          "dante_channels": {"tree": "7_Device_Consoles", "device_path": "+/Channel/+"}}"#;
        let ch = r#"{"Console": {"description": {"En": "Ch ${name}"}, "blocks": {"I": {"fields": {"id": {"device_state": "id"}}}}}}"#;
        let t = parse_template("dante", manifest, TEMPLATE).unwrap();
        let c = parse_template("dante_channels", manifest, ch).unwrap();
        assert_eq!(c.path, vec!["+", "Channel", "+"]);
        let mut b = Book::new("dante", vec![t, c], None);
        let now = Instant::now();
        let dev = "APK.audio/System/Protocols/dante/Device/AVIO-1";
        // A channel leaf is a leaf of the device AND a leaf of the channel.
        let out = b.observe(&format!("{dev}/Channel/3/id"), "3", now);
        let keys: Vec<&str> = out.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(keys, vec!["APK.audio/System/Gui/Panels/dante-AVIO-1", "APK.audio/System/Gui/Panels/dante-AVIO-1-Channel-3"]);
        let doc: Value = serde_json::from_str(&out[1].1).unwrap();
        assert_eq!(doc["placements"].as_array().unwrap().len(), 1, "the channel panel carries only its own family");
        let content = &doc["placements"][0]["frames"][0]["content"]["Console"];
        assert_eq!(content["blocks"]["I"]["fields"]["id"]["topic"], format!("{dev}/Channel/3/id"));
        assert_eq!(doc["folder"], "AVIO-1_3_dante");
        // A channel that goes offline empties only its own panel.
        let out = b.observe(&format!("{dev}/Channel/3/last_online"), "1761830000", now);
        assert_eq!(out, vec![("APK.audio/System/Gui/Panels/dante-AVIO-1-Channel-3".to_string(), String::new())]);
        // Another literal is not a channel.
        assert!(b.observe(&format!("{dev}/Flow/1/id"), "1", now).is_empty());
        for bad in [r#""+/Channel/""#, r#""Channel""#, r#""+/Control""#, "3"] {
            let m = format!(r#"{{"x": {{"device_path": {bad}}}}}"#);
            assert!(parse_template("x", &m, ch).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_device_document_is_the_device() {
        let manifest = r#"{"sap": {"tree": "7_Device_Consoles", "device_depth": 1, "device_document": true}}"#;
        let tpl = r#"{"Console": {"description": {"En": "SAP — ${model}"}, "blocks": {"I": {"fields": {"device": {"device_doc": true}}}}}}"#;
        let t = parse_template("sap", manifest, tpl).unwrap();
        assert!(t.document);
        let mut b = Book::new("sap", vec![t], None);
        let now = Instant::now();
        let dev = "APK.audio/System/Protocols/sap/Device/44_44_44_158";
        let out = b.observe(dev, r#"{"origin": "44.44.44.158", "last_online": "-1", "sessions": {}}"#, now);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "APK.audio/System/Gui/Panels/sap-44_44_44_158");
        let doc: Value = serde_json::from_str(&out[0].1).unwrap();
        let content = &doc["placements"][0]["frames"][0]["content"]["Console"];
        assert_eq!(content["blocks"]["I"]["fields"]["device"]["topic"], dev);
        assert_eq!(content["description"]["En"], "SAP — 44_44_44_158");
        // A leaf under a document device is not the device.
        assert!(b.observe(&format!("{dev}/extra"), "1", now).is_empty());
        // An offline stamp inside the document empties it.
        let out = b.observe(dev, r#"{"last_online": 1761830000}"#, now);
        assert_eq!(out, vec![("APK.audio/System/Gui/Panels/sap-44_44_44_158".to_string(), String::new())]);
        assert!(parse_template("sap", r#"{"sap": {"device_document": "yes"}}"#, tpl).is_err());
    }

    #[test]
    fn the_expiry_is_read_from_the_modules_own_section() {
        let config = json!({"chromecast": {"panels_expire_after_s": "120"}});
        assert_eq!(expire_after_from_config(&config, "chromecast"), Some(Duration::from_secs(120)));
        assert_eq!(expire_after_from_config(&json!({"chromecast": {}}), "chromecast"), None);
    }
}
