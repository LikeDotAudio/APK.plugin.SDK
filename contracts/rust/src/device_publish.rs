// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! A sighting, said as the one document the registry reads.
//!
//! Owns: turning what a hunter can see into the retained DeviceRecord at
//! `APK.audio/Discovery/{protocol}/{deviceId}` — the D2 id derivation, the
//! ISO stamps, the arrival register behind `firstSeen`, and the rule about
//! when a re-announcement is worth putting on the wire.
//!
//! Must not: name an MQTT client. That is the crate rule and it is the reason
//! this module is usable from thirteen agents — [`Sighting::record`] hands
//! back a topic and a payload, and the caller publishes them retained.
//!
//! WHY THIS IS HERE AND NOT IN THE HUNTERS. `Discovery/{protocol}/{deviceId}`
//! is declared in `contracts/src/topics/tree.ts`, subscribed by
//! `Server:Discovery:NMOS/bridge` and, until this module, written by nobody:
//! every hunter published its own field-per-topic spelling under
//! `System/Protocols/{proto}/Device/…` instead, which `element.rs` classifies
//! as an AGENT root and deliberately refuses to register. Thirteen agents
//! reaching that topic separately means thirteen id derivations, and two
//! agents deriving different ids for one instrument is the duplicate-34401A
//! bug `identity.rs` exists to have already fixed once.
//!
//! THE FINGERPRINT EXCLUDES `lastSeen`, AND THAT IS THE WHOLE POINT. A hunter
//! re-announces a device every few seconds for as long as it is on the
//! network. If the moment of the sighting counted as a change, every
//! announcement would be a change, every device would republish at browse
//! rate, and the bridge above would re-register the world — the defect
//! `presence.rs` records against `last_online` carrying `now`, one layer down
//! and with a registry on the end of it. So [`Sighting::record`] answers
//! `None` for a device that has not changed shape, and a stable bench is
//! silent.
//!
//! `firstSeen` COMES FROM [`Presence`], NOT FROM A SECOND REGISTER. The stamp
//! must not move while the device stays up, which is exactly what that type
//! already guarantees for `online_since`; a private `HashMap` here would be a
//! second answer to a question already answered. Retracting through
//! [`Register::retract`] forgets the arrival too, so a device that leaves and
//! comes back gets a new `firstSeen` rather than resuming the old spell.

use std::collections::BTreeMap;

use crate::device_record::{
    DeviceRecord, DeviceRecordExtra, DeviceRecordStatus,
};
use crate::gen::device_record::error::ConversionError;
use crate::identity::device_id_for;
use crate::presence::Presence;
use crate::time::iso_from_unix_ms;
use crate::topics;

/// What a hunter can see about one device, before any of it is a contract.
///
/// Every field is what the protocol actually handed over. Nothing here is
/// derived — `deviceId`, the stamps and the topic are this module's job, and
/// a caller that fills them in itself is the fork this module replaces.
#[derive(Debug, Clone)]
pub struct Sighting {
    /// `appletv`, `chromecast`, `dante`, `visa` — one topic level, and the
    /// prefix of every id derived from it.
    pub protocol: String,
    /// What kind of thing it is, in the hunter's own words. Read by
    /// `element.rs::KEYWORDS` to infer which way a discovered device faces,
    /// so "Speaker Group" earns a receiver and "Microphone" a sender.
    pub device_class: String,
    pub make: String,
    pub model: String,
    /// Where it answers — an IP, a host, a VISA resource string. Also the
    /// identity fallback when the device has no serial.
    pub address: String,
    pub serial: Option<String>,
    pub firmware: Option<String>,
    pub raw_idn: Option<String>,
    pub notes: Option<String>,
    pub extra: Option<DeviceRecordExtra>,
    pub status: DeviceRecordStatus,
}

/// `Default` is written out rather than derived because `DeviceRecordStatus`
/// has none — `gen/` is cargo-typify output and a schema enum has no neutral
/// member. `discovered` is the right zero here anyway, for the reason on
/// [`Sighting::new`].
impl Default for Sighting {
    fn default() -> Self {
        Self {
            protocol: String::new(),
            device_class: String::new(),
            make: String::new(),
            model: String::new(),
            address: String::new(),
            serial: None,
            firmware: None,
            raw_idn: None,
            notes: None,
            extra: None,
            status: DeviceRecordStatus::Discovered,
        }
    }
}

impl Sighting {
    /// A device the hunter can see but has not interrogated.
    ///
    /// `discovered` rather than `identified` is the honest default: a browse
    /// response proves a name resolved, not that anything answered a query.
    /// A hunter that goes on to read a model string promotes it itself.
    pub fn new(protocol: &str, address: &str) -> Self {
        Self {
            protocol: protocol.to_string(),
            address: address.to_string(),
            ..Self::default()
        }
    }

    pub fn class(mut self, class: &str) -> Self {
        self.device_class = class.to_string();
        self
    }

    pub fn made_by(mut self, make: &str, model: &str) -> Self {
        self.make = make.to_string();
        self.model = model.to_string();
        self
    }

    /// The serial, when the protocol gave one. Empty and whitespace-only
    /// strings are dropped rather than carried: `device_id_for` would fall
    /// through them anyway, and an empty `serial` key in the document says
    /// "this device has a serial and it is nothing".
    pub fn serial(mut self, serial: &str) -> Self {
        self.serial = non_empty(serial);
        self
    }

    pub fn firmware(mut self, firmware: &str) -> Self {
        self.firmware = non_empty(firmware);
        self
    }

    pub fn raw_idn(mut self, raw: &str) -> Self {
        self.raw_idn = non_empty(raw);
        self
    }

    pub fn notes(mut self, notes: &str) -> Self {
        self.notes = non_empty(notes);
        self
    }

    pub fn extra(mut self, extra: DeviceRecordExtra) -> Self {
        self.extra = Some(extra);
        self
    }

    pub fn status(mut self, status: DeviceRecordStatus) -> Self {
        self.status = status;
        self
    }

    /// The D2 identity for this sighting: serial, else address, else a hash
    /// of make|model|address. Stable across restarts, and the same value the
    /// TypeScript side derives — `vectors/identity.json` pins that.
    pub fn device_id(&self) -> String {
        device_id_for(
            &self.protocol,
            self.serial.as_deref(),
            Some(&self.address),
            Some(&self.make),
            Some(&self.model),
        )
    }

    /// The document, stamped. Both stamps are `now_ms` — the register in
    /// [`Register`] is what makes `firstSeen` older than that, so a caller
    /// using this directly gets a device that has only ever been seen once.
    pub fn record(&self, now_ms: i64) -> Result<DeviceRecord, String> {
        self.record_seen_since(now_ms, now_ms)
    }

    fn record_seen_since(&self, first_ms: i64, now_ms: i64) -> Result<DeviceRecord, String> {
        let conv = |e: ConversionError| e.to_string();
        Ok(DeviceRecord {
            address: self.address.clone(),
            device_class: self.device_class.clone(),
            device_id: self.device_id().parse().map_err(conv)?,
            extra: self.extra.clone(),
            firmware: self.firmware.clone(),
            first_seen: iso_from_unix_ms(first_ms).parse().map_err(conv)?,
            last_seen: iso_from_unix_ms(now_ms).parse().map_err(conv)?,
            make: self.make.clone(),
            model: self.model.clone(),
            notes: self.notes.clone(),
            protocol: self.protocol.parse().map_err(conv)?,
            raw_idn: self.raw_idn.clone(),
            schema_version: 1.0,
            serial: self.serial.clone(),
            status: self.status,
        })
    }

    /// The retained topic this device's document belongs on.
    pub fn topic(&self) -> Result<String, topics::TopicError> {
        topics::discovery(&self.protocol, &self.device_id())
    }
}

fn non_empty(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() { None } else { Some(t.to_string()) }
}

/// One retained document, ready for the caller's client.
///
/// `payload` is empty for a retraction — an empty retained payload is how MQTT
/// deletes a topic, and every hunter here already retracts its own subtree
/// that way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publication {
    pub topic: String,
    pub payload: String,
}

impl Publication {
    /// Is this the delete rather than a document?
    pub fn is_retraction(&self) -> bool {
        self.payload.is_empty()
    }
}

/// What this agent has already said about each device, so it only speaks when
/// something changed.
///
/// Keyed by `deviceId`, which is derived rather than chosen, so two sightings
/// of one device from two announcements land on one entry however differently
/// the protocol spelled the name that time.
#[derive(Default)]
pub struct Register {
    arrived: Presence,
    /// deviceId → (its topic, fingerprint of the last document published).
    ///
    /// The topic is REMEMBERED rather than rebuilt, because a retraction often
    /// has less to work with than the announcement did: an AVB entity
    /// departing carries an id and nothing else, and a VISA sweep clearing
    /// last scan's rows has no sighting at all. Storing it means every
    /// retraction path names the exact topic that was written, instead of
    /// reconstructing a sighting well enough to re-derive it.
    published: BTreeMap<String, (String, String)>,
}

impl Register {
    pub fn new() -> Self {
        Self { arrived: Presence::new(), published: BTreeMap::new() }
    }

    /// The device, if it is worth saying.
    ///
    /// `Ok(Some(_))` the first time a device is seen and whenever its shape
    /// changes; `Ok(None)` for a re-announcement of something already on the
    /// broker in that exact shape. `Err` when the sighting cannot be made into
    /// a legal document — a protocol with a slash in it, an address that
    /// sanitises to nothing — which is a bug in the caller and not a device
    /// that should be silently dropped.
    pub fn observe(
        &mut self,
        sighting: &Sighting,
        now_ms: i64,
    ) -> Result<Option<Publication>, String> {
        let device_id = sighting.device_id();
        let topic = sighting.topic().map_err(|e| e.to_string())?;
        let first_ms = self.arrived.since(&device_id, now_ms.max(0) as u64) as i64;
        let record = sighting.record_seen_since(first_ms, now_ms)?;

        let print = fingerprint(&record);
        if self.published.get(&device_id).is_some_and(|(_, prev)| *prev == print) {
            return Ok(None);
        }
        self.published.insert(device_id, (topic.clone(), print));

        let payload = serde_json::to_string(&record).map_err(|e| e.to_string())?;
        Ok(Some(Publication { topic, payload }))
    }

    /// The device is gone — clear its retained document.
    ///
    /// `None` when this agent never published it, so a hunter sweeping devices
    /// it has lost does not write a delete for a topic it never wrote. The
    /// arrival stamp is forgotten either way: a device that comes back starts a
    /// new spell rather than claiming it was up across the gap.
    pub fn retract(&mut self, sighting: &Sighting) -> Result<Option<Publication>, String> {
        Ok(self.retract_id(&sighting.device_id()))
    }

    /// The same, for a caller that has the id but cannot rebuild the sighting.
    ///
    /// This is the honest shape for a departure: ADP's `ENTITY_DEPARTING`
    /// carries an entity id and no capabilities, and a VISA sweep clearing the
    /// previous scan has only what it wrote last time. Infallible, because the
    /// topic was validated when it was published — a device on the register is
    /// a device whose topic is already known good.
    pub fn retract_id(&mut self, device_id: &str) -> Option<Publication> {
        self.arrived.forget(device_id);
        let (topic, _) = self.published.remove(device_id)?;
        Some(Publication { topic, payload: String::new() })
    }

    /// Clear every document this agent has published.
    ///
    /// For a hunter whose discovery is a SWEEP rather than a subscription: the
    /// VISA scan deletes the previous pass's retained tree before it starts,
    /// because an instrument that moved category would otherwise linger in
    /// both. Its DeviceRecords have to go the same way and at the same moment,
    /// or the registry keeps a row the Discovered table has already dropped.
    pub fn retract_all(&mut self) -> Vec<Publication> {
        let ids: Vec<String> = self.published.keys().cloned().collect();
        ids.iter().filter_map(|id| self.retract_id(id)).collect()
    }

    /// How many devices this agent currently has documents on the broker for.
    pub fn len(&self) -> usize {
        self.published.len()
    }

    pub fn is_empty(&self) -> bool {
        self.published.is_empty()
    }

    /// Every device this agent has published, for a reconciliation sweep.
    pub fn device_ids(&self) -> Vec<String> {
        self.published.keys().cloned().collect()
    }
}

/// Everything about a document EXCEPT when it was last seen.
///
/// The exclusion is the contract — see the module header. `firstSeen` IS
/// included: it moving means the device left and came back, which is a change
/// a reader should get.
fn fingerprint(r: &DeviceRecord) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
        &*r.device_id,
        &*r.protocol,
        r.device_class,
        r.make,
        r.model,
        r.address,
        r.serial.as_deref().unwrap_or(""),
        r.firmware.as_deref().unwrap_or(""),
        &*r.first_seen,
        r.status,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn appletv() -> Sighting {
        Sighting::new("appletv", "10.0.0.41")
            .class("Media Player")
            .made_by("Apple", "Apple TV 4K")
            .serial("C02ABC123")
    }

    /// The topic is the one the bridge subscribes to, and the id is the D2
    /// derivation rather than anything the protocol chose to call the box.
    #[test]
    fn a_sighting_lands_on_the_topic_the_bridge_reads() {
        let s = appletv();
        assert_eq!(s.device_id(), "appletv:C02ABC123");
        assert_eq!(s.topic().unwrap(), "APK.audio/Discovery/appletv/appletv:C02ABC123");
    }

    /// No serial, so identity falls to the address — and the device is still
    /// one device across announcements rather than a new row each time.
    #[test]
    fn a_device_without_a_serial_is_identified_by_where_it_answers() {
        let s = Sighting::new("chromecast", "10.0.0.77").made_by("Google", "Nest Audio");
        assert_eq!(s.device_id(), "chromecast:10.0.0.77");
    }

    /// The first sighting speaks; an identical re-announcement does not, even
    /// though the moment moved. This is the flood that would otherwise reach
    /// the registry through the bridge.
    #[test]
    fn a_device_that_has_not_changed_says_nothing_again() {
        let mut register = Register::new();
        let s = appletv();
        assert!(register.observe(&s, 1_000_000).unwrap().is_some());
        assert!(register.observe(&s, 1_030_000).unwrap().is_none());
        assert!(register.observe(&s, 9_999_000).unwrap().is_none());
        assert_eq!(register.len(), 1);
    }

    /// A device that actually changes shape does get republished.
    #[test]
    fn a_device_that_changed_is_published_again() {
        let mut register = Register::new();
        assert!(register.observe(&appletv(), 1_000_000).unwrap().is_some());
        let identified = appletv().status(DeviceRecordStatus::Identified);
        assert!(register.observe(&identified, 1_060_000).unwrap().is_some());
    }

    /// `firstSeen` does not move under re-announcement, and `lastSeen` does.
    /// The pair is what a reader dates the device from, so they must not be
    /// the same number forever OR different numbers every browse.
    #[test]
    fn first_seen_holds_while_last_seen_moves() {
        let mut register = Register::new();
        let s = appletv();
        let first = register.observe(&s, 1_000_000).unwrap().unwrap();
        let document: serde_json::Value = serde_json::from_str(&first.payload).unwrap();
        assert_eq!(document["firstSeen"], "1970-01-01T00:16:40.000Z");
        assert_eq!(document["lastSeen"], "1970-01-01T00:16:40.000Z");

        // Something changed, so we publish — and firstSeen is still the arrival.
        let later = register.observe(&s.clone().status(DeviceRecordStatus::Identified), 5_000_000);
        let document: serde_json::Value = serde_json::from_str(&later.unwrap().unwrap().payload).unwrap();
        assert_eq!(document["firstSeen"], "1970-01-01T00:16:40.000Z");
        assert_eq!(document["lastSeen"], "1970-01-01T01:23:20.000Z");
    }

    /// Retraction clears the topic, and only for a device we actually wrote.
    #[test]
    fn retraction_deletes_what_was_written_and_nothing_else() {
        let mut register = Register::new();
        let s = appletv();
        assert!(register.retract(&s).unwrap().is_none(), "never published, nothing to delete");
        register.observe(&s, 1_000_000).unwrap();
        let gone = register.retract(&s).unwrap().unwrap();
        assert!(gone.is_retraction());
        assert_eq!(gone.topic, "APK.audio/Discovery/appletv/appletv:C02ABC123");
        assert!(register.is_empty());
    }

    /// A departure that carries only an id still clears the right topic —
    /// the register remembers what it wrote rather than re-deriving it.
    #[test]
    fn a_device_can_be_retracted_by_id_alone() {
        let mut register = Register::new();
        register.observe(&appletv(), 1_000_000).unwrap();
        let gone = register.retract_id("appletv:C02ABC123").unwrap();
        assert!(gone.is_retraction());
        assert_eq!(gone.topic, "APK.audio/Discovery/appletv/appletv:C02ABC123");
        assert!(register.is_empty());
        assert!(register.retract_id("appletv:C02ABC123").is_none());
    }

    /// A sweep clears everything it wrote, and nothing it did not.
    #[test]
    fn retract_all_clears_the_whole_register() {
        let mut register = Register::new();
        register.observe(&appletv(), 1_000_000).unwrap();
        register.observe(
            &Sighting::new("appletv", "10.0.0.42").made_by("Apple", "HomePod"),
            1_000_000,
        )
        .unwrap();
        assert_eq!(register.len(), 2);
        let cleared = register.retract_all();
        assert_eq!(cleared.len(), 2);
        assert!(cleared.iter().all(|p| p.is_retraction()));
        assert!(register.is_empty());
        assert!(register.retract_all().is_empty());
    }

    /// A device that left and came back is a new spell, not a resumption —
    /// otherwise its `firstSeen` claims it was there across the gap.
    #[test]
    fn a_returning_device_gets_a_new_first_seen() {
        let mut register = Register::new();
        let s = appletv();
        register.observe(&s, 1_000_000).unwrap();
        register.retract(&s).unwrap();
        let back = register.observe(&s, 8_000_000).unwrap().unwrap();
        let document: serde_json::Value = serde_json::from_str(&back.payload).unwrap();
        assert_eq!(document["firstSeen"], "1970-01-01T02:13:20.000Z");
    }

    /// The document validates against the schema the bridge and the registry
    /// read — required fields present, empty optionals omitted rather than
    /// carried as empty strings.
    #[test]
    fn the_payload_is_a_legal_device_record() {
        let mut register = Register::new();
        let out = register.observe(&appletv(), 1_000_000).unwrap().unwrap();
        let document: serde_json::Value = serde_json::from_str(&out.payload).unwrap();
        assert_eq!(document["schemaVersion"], 1.0);
        assert_eq!(document["protocol"], "appletv");
        assert_eq!(document["deviceId"], "appletv:C02ABC123");
        assert_eq!(document["deviceClass"], "Media Player");
        assert_eq!(document["status"], "discovered");
        assert!(document.get("firmware").is_none(), "empty optional is omitted, not empty");
        assert!(document.get("notes").is_none());
        // And it round-trips through the generated validator.
        let parsed: DeviceRecord = serde_json::from_str(&out.payload).unwrap();
        assert_eq!(&*parsed.device_id, "appletv:C02ABC123");
    }

    /// A friendly name full of spaces and slashes is not allowed to fork the
    /// topic tree. Identity sanitises it; the topic builder would reject it.
    #[test]
    fn a_hostile_name_cannot_widen_the_topic_tree() {
        let s = Sighting::new("chromecast", "Anthony's Speaker/+/#")
            .made_by("Google", "Nest");
        let topic = s.topic().unwrap();
        assert_eq!(topic.matches('/').count(), 3, "still four levels");
        assert!(!topic.contains('+') && !topic.contains('#'));
    }

    /// A caller that hands over something unusable gets told, rather than
    /// having the device quietly vanish from the registry.
    #[test]
    fn an_illegal_protocol_is_an_error_not_a_dropped_device() {
        let mut register = Register::new();
        let s = Sighting::new("not/a/protocol", "10.0.0.1");
        assert!(register.observe(&s, 1_000).is_err());
    }
}
