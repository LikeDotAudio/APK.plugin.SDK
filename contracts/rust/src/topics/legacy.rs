// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Part of `topics.rs` — see that file for the whole picture.

use super::*;

pub(super) fn classify_legacy(segs: &[&str]) -> Option<LegacyTopic> {
    let s1 = segs.get(1).copied();
    let s2 = segs.get(2).copied();
    let s3 = segs.get(3).copied();
    let rest: Vec<&str> = segs.iter().skip(4).copied().collect();

    if s1 == Some("Protocol") {
        if let Some(channel) = s2 {
            let mut t = LegacyTopic::new("wsSideBus");
            t.channel = Some(channel.to_string());
            t.segments = Some(segs.iter().skip(3).map(|s| s.to_string()).collect());
            return Some(t);
        }
        return None;
    }
    if s1 != Some("System") {
        return None;
    }
    if s2 == Some("Failover") && s3 == Some("WEB") && rest.len() == 2 && rest[0] == "Heartbeat" {
        let mut t = LegacyTopic::new("failoverWebHeartbeat");
        t.guid = Some(rest[1].to_string());
        return Some(t);
    }
    if s2 != Some("Protocols") {
        return None;
    }
    let proto = s3?;
    if proto == "visa" && rest.first() == Some(&"Device") {
        let mut t = LegacyTopic::new("visaDeviceTree");
        t.segments = Some(rest.iter().skip(1).map(|s| s.to_string()).collect());
        return Some(t);
    }
    if proto == "midi" && rest.first() == Some(&"Device") {
        let mut t = LegacyTopic::new("midiDeviceTree");
        t.segments = Some(rest.iter().skip(1).map(|s| s.to_string()).collect());
        return Some(t);
    }
    if proto == "komand" {
        let mut t = LegacyTopic::new("komandAgent");
        t.channel = Some(rest.join("/"));
        return Some(t);
    }
    if rest.is_empty() || rest == ["status"] {
        let mut t = LegacyTopic::new("protocolStatus");
        t.protocol = Some(proto.to_string());
        return Some(t);
    }
    if rest == ["config"] {
        let mut t = LegacyTopic::new("protocolConfig");
        t.protocol = Some(proto.to_string());
        return Some(t);
    }
    None
}
