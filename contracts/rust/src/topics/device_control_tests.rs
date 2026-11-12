// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The tests of `topics.rs`, out of line.

use super::*;

/// The shape fourteen consoles and two agents already depend on, now
/// composed in one place instead of spelled in four. PLAN-914.01.
#[test]
fn a_press_addresses_one_device_own_agent() {
    assert_eq!(
        device_control("chromecast", "Video_Cast", "Living_Room", "Volume").unwrap(),
        "APK.audio/System/Protocols/chromecast/Device/Video_Cast/Living_Room/Control/Volume"
    );
    assert_eq!(
        device_control("appletv", "Video", "Apple_TV_HD__4th_gen__44-44-44-150", "Play").unwrap(),
        "APK.audio/System/Protocols/appletv/Device/Video/Apple_TV_HD__4th_gen__44-44-44-150/Control/Play"
    );
}

/// THE GAIN OVER THE FREE TAIL. `protocols()` takes `&[&str]` and validates
/// each segment, but nothing made the caller pass four of them in this
/// order — a category carrying a separator used to build a longer topic and
/// fail by being subscribed to by nobody. It is a `TopicError` now, at the
/// moment the topic is composed.
#[test]
fn a_segment_carrying_a_separator_is_refused_rather_than_published() {
    for bad in ["Video/Cast", "Video+Cast", "Video#Cast", ""] {
        assert!(device_control("chromecast", bad, "Living_Room", "Volume").is_err(),
                "category {bad:?} was allowed through");
        assert!(device_control("chromecast", "Video_Cast", bad, "Volume").is_err(),
                "device {bad:?} was allowed through");
        assert!(device_control("chromecast", "Video_Cast", "Living_Room", bad).is_err(),
                "verb {bad:?} was allowed through");
    }
}

/// One device, every verb — and `#` rather than `+`, so a deeper verb
/// tomorrow still reaches the agent that owns the device.
#[test]
fn the_wildcard_covers_one_device_and_stops_there() {
    let sub = device_control_wildcard("chromecast", "Video_Cast", "Living_Room").unwrap();
    assert_eq!(sub, "APK.audio/System/Protocols/chromecast/Device/Video_Cast/Living_Room/Control/#");
    let press = device_control("chromecast", "Video_Cast", "Living_Room", "Mute").unwrap();
    assert!(press.starts_with(sub.trim_end_matches('#')),
            "the wildcard does not cover the topic the builder writes");
}

/// The leaf is the string the two vendor crates match on and the console
/// generator writes. It is asserted rather than assumed, because changing it
/// here without changing them is exactly the drift this constant exists to
/// stop.
/// The filter covers what the VISA agent publishes for one instrument:
/// `{prefix}/Reply`, the prefix being three levels below `Device`.
#[test]
fn the_visa_reply_wildcard_covers_one_reply() {
    let sub = visa_reply_wildcard();
    assert_eq!(sub, "APK.audio/System/Protocols/visa/Device/+/+/+/Reply");
    let reply = protocols("visa", &["Device", "Spectrum", "N9340B", "Dev0", DEVICE_REPLY_LEAF]).unwrap();
    let (s, r): (Vec<&str>, Vec<&str>) = (sub.split('/').collect(), reply.split('/').collect());
    assert_eq!(s.len(), r.len());
    assert!(s.iter().zip(&r).all(|(a, b)| *a == "+" || a == b));
}

#[test]
fn the_leaf_is_the_word_the_agents_match_on() {
    assert_eq!(DEVICE_CONTROL_LEAF, "Control");
}
