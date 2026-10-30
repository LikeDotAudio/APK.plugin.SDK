// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! KomandClockArmed — which controls are reading the bench clock out loud.
//!
//! `APK.audio/Komand/Clock/Armed`, retained, one document: the agent's own
//! record of what it is doing, so that restarting it does not silently undo
//! it. `armed` was a `HashMap` in `mqtt.rs`'s stack frame and nothing outlived
//! the process, while the tick boxes that armed it are retained and stayed
//! ticked — an instrument left emitting a valid frequency that had stopped
//! changing, and no way to tell but to watch the number. PLAN-736.01.
//!
//! WHY THE ARM COULD NOT SIMPLY BE READ BACK OFF THE TOPIC THAT SET IT
//!
//! Arming is published on a `gui` topic, and KOMAND refuses to act on a
//! RETAINED value there — *"retained replay on {} — state, not a command"* —
//! which is the rule that stops a reconnect re-sending whatever each panel was
//! last set to, `*RST` at every discovered instrument included. So an arming
//! word must go out NON-retained and is gone the instant it is delivered. This
//! is the state half, on a topic of its own, so that nothing about the
//! retained-command refusal has to move to fix the forgetting.
//!
//! THE CLEAR IS PART OF THE CONTRACT, not a courtesy. A retained value that
//! outlives the thing it describes is the mirror bug, and this repository is
//! six instances deep in it. `publication()` on an empty ledger returns a
//! ZERO-LENGTH payload, which is how MQTT deletes a retained topic — the same
//! move `agent_state`'s `interface` leaf makes when an agent that was
//! listening on `eth0` goes dark. An agent with nothing armed must not leave a
//! generator's name standing on the bus.
//!
//! NO INSTANTS. The document carries what is armed, on which word, and the
//! write that armed it — and nothing that goes stale by sitting still. That is the `timeMark` trap read
//! the other way round: a retained document is served to a subscriber an hour
//! later, so every field in one has to be a fact that is still true then.
//!
//! THE WORD IS OPAQUE HERE. `tod:1s` and its four siblings are declared in
//! `APK:audio:komand/src/time_of_day.rs::read_arm` and held to the widget by
//! `check.sh clocksync`, which already compares four statements of that
//! vocabulary. A fifth, here, would be one more thing to drift; a ledger entry
//! carries the word through and the agent parses it with the same `read_arm`
//! that admitted it in the first place.
//!
//! Rust-only, like `agent_state` and for its reason: the only publisher and
//! the only reader of this topic is one Rust binary, and a zod schema with no
//! TypeScript consumer is a second shape to keep in step for nobody.

use serde::{Deserialize, Serialize};

/// The document's version. Bumped only when a reader that ignores unknown
/// fields could no longer do the right thing with the old shape.
pub const SCHEMA_VERSION: u32 = 1;

/// One control that is following the clock: the GUI topic that armed it, the
/// word it was armed with, and the write that armed it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClockArm {
    /// The GUI topic the arming write landed on — the same key the agent's
    /// live map uses, and the only thing that tells two generators apart.
    pub topic: String,
    /// The arming word, verbatim. See the module docs: this contract does not
    /// know the vocabulary and must not learn it.
    pub word: String,
    /// THE WRITE THAT ARMED THE CONTROL, carried whole and opaque.
    ///
    /// Without it the ledger restores nothing on the bench it exists for. A
    /// control's handler reaches KOMAND on `<topic>/config`, and that config is
    /// published by the WIDGET on mount — so an agent restarting with no
    /// browser open learns which instrument the arm was about from nowhere at
    /// all, and the hour chime is precisely the case where nobody is watching.
    /// Snapshotting the write makes the ledger self-sufficient.
    ///
    /// It is a snapshot and is treated as one: a LIVE config, if the agent has
    /// since cached one for this topic, is more current than a document written
    /// before the last restart and wins.
    #[serde(default)]
    pub message: serde_json::Value,
}

impl ClockArm {
    /// An arm with no snapshot — restorable only where a live config has
    /// already been cached for the topic.
    pub fn new(topic: &str, word: &str) -> Self {
        ClockArm {
            topic: topic.to_string(),
            word: word.to_string(),
            message: serde_json::Value::Null,
        }
    }

    pub fn carrying(topic: &str, word: &str, message: serde_json::Value) -> Self {
        ClockArm { topic: topic.to_string(), word: word.to_string(), message }
    }

    /// Is there a snapshot to rebuild from, or must a live config supply it?
    pub fn has_snapshot(&self) -> bool {
        !self.message.is_null()
    }
}

/// One control whose arm was TAKEN by another panel arming the same instrument
/// input, and which is therefore drawing ticked boxes over a knob it no longer
/// drives.
///
/// WHY THIS IS ON THE LEDGER AND NOT A CLEAR OF THE OPERATOR'S BOXES. Every
/// generator on this bench is drawn twice from one template, so the arm the
/// "one input, one clock" rule drops always belongs to a panel somebody may be
/// looking at (PLAN-1027.01) — and that panel's tick boxes are retained on
/// their own topics, bound to no `komand_handler`, which is the very property
/// that lets KOMAND READ them without the retained-command question arising.
/// The agent clearing them would make it an AUTHOR of the operator's own
/// record, and the next restore would then have no veto to ask (PLAN-736.01).
/// So the agent says who lost, here, on the document it already owns, and the
/// widget draws the difference. PLAN-3303.01.
///
/// EVERY FIELD IS STILL TRUE AN HOUR LATER, which is the rule for anything
/// retained: `topic` is the panel that lost, `by` is the GUI topic that holds
/// the instrument input now, and `word` is what that winner is armed with. No
/// instant, no count, nothing that goes stale by sitting still — and the whole
/// entry disappears the moment `by` stops being armed, because `displacing`
/// below refuses to carry one whose winner is not in `armed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Displaced {
    /// The GUI topic whose arm was dropped — the panel with the stale boxes.
    pub topic: String,
    /// The GUI topic that is following the clock on that instrument input now.
    pub by: String,
    /// The winner's arming word, verbatim and opaque, exactly as `ClockArm`
    /// carries one. A panel can then say what the control IS doing, not only
    /// that it is no longer doing what its own boxes claim.
    pub word: String,
}

impl Displaced {
    pub fn new(topic: &str, by: &str, word: &str) -> Self {
        Displaced { topic: topic.to_string(), by: by.to_string(), word: word.to_string() }
    }
}

/// The whole retained document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArmedLedger {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub armed: Vec<ClockArm>,
    /// The panels whose arms were taken. `#[serde(default)]` because a document
    /// written by an agent older than PLAN-3303.01 has no such key, and a reader
    /// that could not open one would refuse the whole ledger over a field that
    /// only decorates a panel.
    #[serde(default)]
    pub displaced: Vec<Displaced>,
}

impl ArmedLedger {
    /// Build a ledger from whatever the agent currently holds armed.
    ///
    /// SORTED BY TOPIC, so the same set of armed controls always serialises to
    /// the same bytes. The agent republishes on every arm and disarm and a
    /// `HashMap`'s iteration order is not stable between runs; without this,
    /// re-publishing an unchanged ledger would look to every subscriber like a
    /// change, and a diff of two captures would be unreadable.
    pub fn of(arms: impl IntoIterator<Item = ClockArm>) -> Self {
        let mut armed: Vec<ClockArm> = arms.into_iter().collect();
        armed.sort_by(|a, b| a.topic.cmp(&b.topic));
        ArmedLedger { schema_version: SCHEMA_VERSION, armed, displaced: Vec::new() }
    }

    /// Name the panels whose arms were taken, on the ledger just built.
    ///
    /// A SEPARATE STEP RATHER THAN A SECOND ARGUMENT TO `of`, because the
    /// displacements are only meaningful against a set of arms that is already
    /// decided — and this is where that is checked. AN ENTRY WHOSE WINNER IS
    /// NOT ARMED IS DROPPED HERE: the loser's boxes are only overridden for as
    /// long as something else is holding the instrument input, so a
    /// displacement that outlived its displacer would be the retained-value
    /// mirror bug wearing a new hat, and this contract is not going to publish
    /// one. Sorted by topic for the same reason `armed` is.
    pub fn displacing(mut self, displaced: impl IntoIterator<Item = Displaced>) -> Self {
        let held: std::collections::HashSet<&str> =
            self.armed.iter().map(|a| a.topic.as_str()).collect();
        let mut rows: Vec<Displaced> =
            displaced.into_iter().filter(|d| held.contains(d.by.as_str())).collect();
        rows.sort_by(|a, b| a.topic.cmp(&b.topic));
        self.displaced = rows;
        self
    }

    /// Nothing armed. `displaced` is deliberately NOT consulted: `displacing`
    /// has already dropped every entry whose winner is not armed, so an empty
    /// `armed` can carry no displacements at all — and if it somehow did, the
    /// right thing is still to clear the topic rather than publish a document
    /// about a clock nothing is following.
    pub fn is_empty(&self) -> bool {
        self.armed.is_empty()
    }

    /// Everything one publish call needs: topic, bytes, retain.
    ///
    /// `retain` is the contract's answer and never the call site's, the same
    /// split `agent_state` and `agent_counters` make in opposite directions: a
    /// ledger that is not retained tells an agent that reconnects after a crash
    /// exactly nothing, which is the whole defect.
    ///
    /// EMPTY MEANS DELETE. An empty ledger serialises to zero bytes rather
    /// than to `{"armed":[]}`, so the retained topic is removed from the broker
    /// instead of standing there as an empty document forever.
    pub fn publication(&self) -> (String, Vec<u8>, bool) {
        let topic = crate::topics::komand_clock_armed();
        if self.is_empty() {
            return (topic, Vec::new(), true);
        }
        // A failure here is a `message` snapshot that will not re-serialise,
        // which serde_json's own value tree makes very nearly impossible. It
        // must not lose the ledger silently either way, so it degrades to the
        // clear an empty ledger would have sent rather than to a stale
        // document — a topic with nothing on it is readable as nothing armed,
        // and a topic carrying yesterday's arms is not.
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        (topic, bytes, true)
    }

    /// The consumer's half, and deliberately tolerant: an unreadable or empty
    /// payload is NOTHING ARMED, never an error the caller has to invent a
    /// policy for. A cleared topic arrives as zero bytes, which is the normal
    /// way this reads on a bench where nobody has armed anything.
    ///
    /// An entry missing either half is DROPPED rather than defaulted: an arm
    /// with no topic names no control, and an arm with no word cannot be
    /// resolved to a tier by anything downstream.
    pub fn read(payload: &[u8]) -> Vec<ClockArm> {
        if payload.is_empty() {
            return Vec::new();
        }
        let doc: serde_json::Value = match serde_json::from_slice(payload) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        let mut out = Vec::new();
        for entry in doc.get("armed").and_then(|v| v.as_array()).unwrap_or(&Vec::new()) {
            let topic = entry.get("topic").and_then(|v| v.as_str()).unwrap_or("");
            let word = entry.get("word").and_then(|v| v.as_str()).unwrap_or("");
            if topic.is_empty() || word.is_empty() {
                continue;
            }
            // A MISSING SNAPSHOT IS NOT A MISSING ARM. `message` is absent in
            // any document an older agent wrote, and the entry is still worth
            // restoring wherever a live config has arrived for the topic — so
            // it defaults to null and the caller decides, rather than being
            // dropped here for a field that was optional when it was written.
            let message = entry.get("message").cloned().unwrap_or(serde_json::Value::Null);
            out.push(ClockArm::carrying(topic, word, message));
        }
        out
    }

    /// The displaced half, read with the same tolerance as `read`: an
    /// unreadable payload, an absent key — the shape every document written
    /// before PLAN-3303.01 has — or an entry missing any of its three fields is
    /// simply nothing displaced. A panel drawing itself as overridden on a
    /// half-read entry would be worse than one drawing itself normally.
    pub fn read_displaced(payload: &[u8]) -> Vec<Displaced> {
        if payload.is_empty() {
            return Vec::new();
        }
        let doc: serde_json::Value = match serde_json::from_slice(payload) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        let mut out = Vec::new();
        for entry in doc.get("displaced").and_then(|v| v.as_array()).unwrap_or(&Vec::new()) {
            let field = |k: &str| entry.get(k).and_then(|v| v.as_str()).unwrap_or("");
            let (topic, by, word) = (field("topic"), field("by"), field("word"));
            if topic.is_empty() || by.is_empty() || word.is_empty() {
                continue;
            }
            out.push(Displaced::new(topic, by, word));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_family_is_one_exact_topic() {
        let (topic, _, _) = ArmedLedger::of([]).publication();
        assert_eq!(topic, "APK.audio/Komand/Clock/Armed");
    }

    /// The reason the family exists: a ledger that is not retained is a ledger
    /// an agent that has just come back cannot read.
    #[test]
    fn a_ledger_is_always_retained_including_the_clear() {
        assert!(ArmedLedger::of([]).publication().2);
        assert!(ArmedLedger::of([ClockArm::new("APK.audio/Gui/A/f", "tod:1s")]).publication().2);
    }

    /// Nothing armed CLEARS the topic. An empty document left standing is the
    /// mirror bug: a retained value that outlives what it describes.
    #[test]
    fn nothing_armed_publishes_zero_bytes_which_is_how_mqtt_deletes() {
        let (_, bytes, retain) = ArmedLedger::of([]).publication();
        assert!(bytes.is_empty(), "an empty ledger must clear, not publish {{armed:[]}}");
        assert!(retain);
    }

    #[test]
    fn a_ledger_round_trips() {
        let ledger = ArmedLedger::of([
            ClockArm::new("APK.audio/Gui/Bench/Gen2/Frequency_Presets", "tod:1h"),
            ClockArm::new("APK.audio/Gui/Bench/Gen1/Frequency_Presets", "tod:1s"),
        ]);
        let (_, bytes, _) = ledger.publication();
        let back = ArmedLedger::read(&bytes);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].word, "tod:1s");
        assert_eq!(back[1].word, "tod:1h");
    }

    /// Two runs holding the same arms must publish the same bytes, or every
    /// republish reads as a change to everything downstream.
    #[test]
    fn the_bytes_do_not_depend_on_the_order_the_agent_happened_to_hold_them_in() {
        let one = ArmedLedger::of([
            ClockArm::new("APK.audio/Gui/b", "tod:1m"),
            ClockArm::new("APK.audio/Gui/a", "tod:1s"),
        ]);
        let two = ArmedLedger::of([
            ClockArm::new("APK.audio/Gui/a", "tod:1s"),
            ClockArm::new("APK.audio/Gui/b", "tod:1m"),
        ]);
        assert_eq!(one.publication().1, two.publication().1);
    }

    /// A cleared retained topic arrives as zero bytes, and that is the ordinary
    /// case on a bench where nothing is armed — not an error.
    #[test]
    fn a_cleared_topic_reads_as_nothing_armed() {
        assert!(ArmedLedger::read(b"").is_empty());
    }

    /// A displacement is carried, and it survives the round trip whole.
    #[test]
    fn the_ledger_names_the_panel_whose_arm_was_taken() {
        let ledger = ArmedLedger::of([ClockArm::new("APK.audio/Gui/Console/Gen1/Freq", "tod:1h")])
            .displacing([Displaced::new(
                "APK.audio/Gui/Instruments/Gen1/Freq",
                "APK.audio/Gui/Console/Gen1/Freq",
                "tod:1h",
            )]);
        let (_, bytes, _) = ledger.publication();
        let back = ArmedLedger::read_displaced(&bytes);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].topic, "APK.audio/Gui/Instruments/Gen1/Freq");
        assert_eq!(back[0].by, "APK.audio/Gui/Console/Gen1/Freq");
        assert_eq!(back[0].word, "tod:1h");
    }

    /// The invariant the whole field rests on: boxes are only overridden while
    /// something else is holding the instrument input. A displacement naming a
    /// winner that is not armed is not published at all.
    #[test]
    fn a_displacement_whose_winner_is_not_armed_is_dropped() {
        let ledger = ArmedLedger::of([ClockArm::new("APK.audio/Gui/other", "tod:1s")])
            .displacing([Displaced::new("APK.audio/Gui/loser", "APK.audio/Gui/gone", "tod:1h")]);
        assert!(ledger.displaced.is_empty());
        assert!(ArmedLedger::read_displaced(&ledger.publication().1).is_empty());
    }

    /// A document written before this field existed still opens, and reads as
    /// nothing displaced rather than as an unreadable ledger.
    #[test]
    fn a_ledger_with_no_displaced_key_still_reads() {
        let old = br#"{"schemaVersion":1,"armed":[{"topic":"APK.audio/Gui/a","word":"tod:1s"}]}"#;
        assert_eq!(ArmedLedger::read(old).len(), 1);
        assert!(ArmedLedger::read_displaced(old).is_empty());
    }

    /// Tolerant reader (contracts §2.1). Rubbish on the topic must not take the
    /// agent down; it means the same as nothing armed.
    #[test]
    fn an_unreadable_payload_is_nothing_armed_rather_than_a_panic() {
        assert!(ArmedLedger::read(b"not json at all").is_empty());
        assert!(ArmedLedger::read(b"{}").is_empty());
        assert!(ArmedLedger::read(br#"{"armed":"a string"}"#).is_empty());
    }

    /// A half-written entry names no control, or names one with no word to
    /// re-arm it by. Either way there is nothing to restore, so it is dropped
    /// rather than defaulted into an arm nobody asked for.
    #[test]
    fn a_half_written_entry_is_dropped_not_defaulted() {
        let arms = ArmedLedger::read(
            br#"{"schemaVersion":1,"armed":[
                 {"topic":"APK.audio/Gui/a"},
                 {"word":"tod:1s"},
                 {"topic":"APK.audio/Gui/b","word":"tod:1m"}]}"#,
        );
        assert_eq!(arms, vec![ClockArm::new("APK.audio/Gui/b", "tod:1m")]);
    }

    /// The word travels through unexamined. This contract does not hold the
    /// arming vocabulary — `time_of_day.rs` does, and `check.sh clocksync`
    /// holds it to the widget — so an unknown word must reach the agent to be
    /// refused there rather than be swallowed here.
    #[test]
    fn the_arming_word_is_carried_opaquely() {
        let arms = ArmedLedger::read(
            br#"{"schemaVersion":1,"armed":[{"topic":"APK.audio/Gui/a","word":"tod:7q"}]}"#,
        );
        assert_eq!(arms[0].word, "tod:7q");
    }

    /// The snapshot is what makes the ledger work on a bench with no browser
    /// open: the handler reaches KOMAND on `<topic>/config`, published by the
    /// widget on mount, and the hour chime is the case where nobody is looking.
    #[test]
    fn the_write_that_armed_the_control_round_trips_whole() {
        let msg = serde_json::json!({"value": "tod:1h", "komand_handler": {"input_name": "freq"}});
        let ledger = ArmedLedger::of([ClockArm::carrying("APK.audio/Gui/a", "tod:1h", msg.clone())]);
        let back = ArmedLedger::read(&ledger.publication().1);
        assert_eq!(back[0].message, msg);
        assert!(back[0].has_snapshot());
    }

    /// An entry an OLDER agent wrote carries no `message`, and that is not a
    /// reason to forget the arm — a live config may well supply the handler.
    /// It is handed on with the snapshot missing and the caller decides.
    #[test]
    fn an_entry_with_no_snapshot_survives_the_read_and_says_so() {
        let arms = ArmedLedger::read(
            br#"{"schemaVersion":1,"armed":[{"topic":"APK.audio/Gui/a","word":"tod:1s"}]}"#,
        );
        assert_eq!(arms.len(), 1);
        assert!(!arms[0].has_snapshot());
    }

    /// An unknown field must not throw the document away: the reader is
    /// tolerant by policy, so a newer agent's extra key is ignored and the
    /// arms still restore.
    #[test]
    fn an_unknown_field_does_not_lose_the_arms() {
        let arms = ArmedLedger::read(
            br#"{"schemaVersion":2,"armed":[{"topic":"APK.audio/Gui/a","word":"tod:1s","tier":"x"}],"note":"hi"}"#,
        );
        assert_eq!(arms, vec![ClockArm::new("APK.audio/Gui/a", "tod:1s")]);
    }
}
