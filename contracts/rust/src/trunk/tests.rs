// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The tests of `trunk.rs`, out of line.

use super::*;

#[test]
fn a_line_round_trips() {
    let s = "HQ,dante-01,U00,Andrey,CLOSE,Sam,U05,sip-03,HQ";
    let l = Line::parse(s).unwrap();
    assert_eq!(l.render(), s);
    assert!(!l.is_local());
    assert!(!l.flipped().close);
}

#[test]
fn a_batcher_flushes_at_the_window_at_the_cap_and_at_the_end() {
    use std::time::{Duration, Instant};
    let t0 = Instant::now();
    let ms = |n: u64| t0 + Duration::from_millis(n);
    let mut b: Batcher = Batcher::new(Duration::from_millis(100), 4);
    // Three entries inside one window: nothing before it, ONE batch of three at it.
    for (n, at) in [(0, 0), (1, 30), (2, 90)] {
        assert!(b.push("f1", format!("l{n}"), ms(at)).is_none());
    }
    assert!(b.due(ms(99)).is_empty());
    assert_eq!(b.due(ms(100)), vec![("f1".to_string(), vec!["l0".to_string(), "l1".into(), "l2".into()])]);
    assert_eq!(b.pending(), 0);
    // The cap hands the batch back at once; the next entry opens a new one.
    for n in 0..3 {
        assert!(b.push("f2", format!("c{n}"), ms(200)).is_none());
    }
    assert_eq!(b.push("f2", "c3".into(), ms(201)).unwrap().1.len(), 4);
    assert!(b.push("f2", "c4".into(), ms(202)).is_none());
    // Topics are separate batches; the final partial ones are drained.
    assert!(b.push("f3", "x".into(), ms(203)).is_none());
    let mut left = b.drain();
    left.sort();
    assert_eq!(left, vec![("f2".to_string(), vec!["c4".to_string()]), ("f3".to_string(), vec!["x".to_string()])]);
    assert!(b.drain().is_empty());
    assert_eq!(Batcher::<String>::new(Duration::ZERO, 10_000).cap, BATCH_MAX);
}

/// 0.7.0: (frame, line) entries for many frames of one protocol leave as
/// ONE batch, each frame's lines in push order, and read back per frame.
#[test]
fn a_protocol_batch_keeps_each_frames_order() {
    use std::time::{Duration, Instant};
    let t0 = Instant::now();
    let mut b: Batcher<(String, String)> = Batcher::new(Duration::from_millis(100), 200);
    let topic = protocol_ack_topic("dante").unwrap().0;
    for (f, l) in [("dante-02", "a"), ("dante-01", "b"), ("dante-02", "c"), ("dante-01", "d")] {
        assert!(b.push(&topic, (f.to_string(), l.to_string()), t0).is_none());
    }
    let due = b.due(t0 + Duration::from_millis(100));
    assert_eq!(due.len(), 1, "one message for the protocol");
    let frames = by_frame(due.into_iter().next().unwrap().1);
    let back = parse_protocol_ack_batch(&render_protocol_ack_batch(&frames)).unwrap();
    assert_eq!(back["dante-01"], vec!["b".to_string(), "d".into()]);
    assert_eq!(back["dante-02"], vec!["a".to_string(), "c".into()]);
    assert_eq!(parse_sim_protocol_topic(&topic), Some(("dante".into(), "ack".into())));
}

#[test]
fn every_verb_is_one_route() {
    let mut routes: Vec<&str> = VERBS.iter().map(|v| v.route).collect();
    routes.sort_unstable();
    routes.dedup();
    assert_eq!(routes.len(), VERBS.len());
}

#[test]
fn the_mailbox_leaves_are_the_contracts() {
    use crate::baremetal_io::{module_incoming, module_outgoing};
    assert_eq!(module_outgoing(TOKEN, "command").unwrap(), command_topic());
    assert_eq!(module_incoming(TOKEN, "result").unwrap(), result_topic());
}
