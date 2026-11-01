// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The trunk vectors (ROAD-3440), shared with the Python and TypeScript twins.

use apkaudio_contracts::trunk::*;
use serde_json::Value;
use std::path::PathBuf;

fn vectors() -> Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vectors/trunk.json");
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap()
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap()
}

#[test]
fn lines_parse_render_and_classify() {
    let v = vectors();
    for case in v["lines"]["valid"].as_array().unwrap() {
        let l = Line::parse(s(&case["text"])).unwrap_or_else(|e| panic!("{}: {e}", case["why"]));
        assert_eq!(l.to_value(), case["line"], "{}", case["why"]);
        assert_eq!(l.render(), s(&case["render"]), "{}", case["why"]);
        assert_eq!(Line::parse(&l.render()).unwrap(), l, "{}", case["why"]);
        assert_eq!(l.is_local(), case["local"].as_bool().unwrap(), "{}", case["why"]);
        assert_eq!(l.is_checkin(), case["checkin"].as_bool().unwrap(), "{}", case["why"]);
        assert_eq!(l.flipped().flipped(), l);
    }
    for case in v["lines"]["invalid"].as_array().unwrap() {
        assert!(Line::parse(s(&case["text"])).is_err(), "{}", case["why"]);
    }
    for case in v["checkins"].as_array().unwrap() {
        let l = Line::checkin(s(&case["site"]), s(&case["frame"]), s(&case["when"]), case["closed"].as_u64().unwrap() as usize);
        assert_eq!(l.render(), s(&case["text"]));
    }
    for case in v["rejects"].as_array().unwrap() {
        assert_eq!(render_reject(s(&case["reason"]), s(&case["line"])), s(&case["payload"]));
        let (reason, line) = parse_reject(s(&case["payload"])).unwrap();
        assert_eq!(line, s(&case["line"]));
        assert!(!reason.contains('|'));
    }
}

#[test]
fn line_and_ack_batches() {
    let v = vectors();
    let b = &v["batches"];
    assert_eq!(BATCH_MAX as u64, b["max"].as_u64().unwrap());
    for case in b["valid"].as_array().unwrap() {
        let entries: Vec<String> = case["entries"].as_array().unwrap().iter().map(|e| s(e).to_string()).collect();
        let line = s(&case["leaf"]) == "line";
        let render = if line { render_line_batch } else { render_ack_batch };
        let parse = if line { parse_line_batch } else { parse_ack_batch };
        if let Some(p) = case["payload"].as_str() {
            assert_eq!(render(&entries), p, "{}", case["why"]);
        }
        let payload = case["read"].as_str().or(case["payload"].as_str()).unwrap();
        assert_eq!(parse(payload).unwrap_or_else(|e| panic!("{}: {e}", case["why"])), entries, "{}", case["why"]);
    }
    for case in b["invalid"].as_array().unwrap() {
        let parse = if s(&case["leaf"]) == "line" { parse_line_batch } else { parse_ack_batch };
        assert!(parse(s(&case["payload"])).is_err(), "{}", case["why"]);
    }
    let many: Vec<String> = (0..=BATCH_MAX).map(|n| format!("HQ,f,U{n:02},A,CLOSE,B,U01,f,HQ")).collect();
    assert!(parse_line_batch(&render_line_batch(&many)).is_err(), "more than BATCH_MAX is refused");
    assert_eq!(parse_line_batch(&render_line_batch(&many[..BATCH_MAX])).unwrap().len(), BATCH_MAX);
}

fn frames_of(v: &Value) -> std::collections::BTreeMap<String, Vec<String>> {
    v.as_object().unwrap().iter().map(|(k, l)| (k.clone(), l.as_array().unwrap().iter().map(|e| s(e).to_string()).collect())).collect()
}

#[test]
fn protocol_batches_and_states() {
    let v = vectors();
    let b = &v["protocol_batches"];
    assert_eq!(BATCH_MAX as u64, b["max"].as_u64().unwrap());
    for case in b["valid"].as_array().unwrap() {
        let frames = frames_of(&case["frames"]);
        let line = s(&case["leaf"]) == "line";
        let render = if line { render_protocol_line_batch } else { render_protocol_ack_batch };
        let parse = if line { parse_protocol_line_batch } else { parse_protocol_ack_batch };
        if let Some(p) = case["payload"].as_str() {
            assert_eq!(render(&frames), p, "{}", case["why"]);
        }
        let payload = case["read"].as_str().or(case["payload"].as_str()).unwrap();
        assert_eq!(parse(payload).unwrap_or_else(|e| panic!("{}: {e}", case["why"])), frames, "{}", case["why"]);
    }
    for case in b["invalid"].as_array().unwrap() {
        let parse = if s(&case["leaf"]) == "line" { parse_protocol_line_batch } else { parse_protocol_ack_batch };
        assert!(parse(s(&case["payload"])).is_err(), "{}", case["why"]);
    }
    // BATCH_MAX counts entries across every frame.
    let many: std::collections::BTreeMap<String, Vec<String>> =
        (0..2).map(|f| (format!("f-{f}"), (0..BATCH_MAX / 2).map(|n| format!("HQ,f,U{n:02},A,CLOSE,B,U01,f,HQ")).collect())).collect();
    assert!(parse_protocol_line_batch(&render_protocol_line_batch(&many)).is_ok());
    let mut over = many.clone();
    over.insert("f-9".into(), vec!["HQ,f,U01,A,CLOSE,B,U01,f,HQ".into()]);
    assert!(parse_protocol_line_batch(&render_protocol_line_batch(&over)).is_err(), "more than BATCH_MAX across frames is refused");
    let st = &v["protocol_states"];
    for case in st["valid"].as_array().unwrap() {
        let frames: std::collections::BTreeMap<String, Value> = case["frames"].as_object().unwrap().iter().map(|(k, x)| (k.clone(), x.clone())).collect();
        let want = (s(&case["protocol"]).to_string(), frames.clone());
        assert_eq!(parse_protocol_state(s(&case["payload"])).unwrap(), want, "{}", case["why"]);
        assert_eq!(parse_protocol_state(&render_protocol_state(s(&case["protocol"]), &frames)).unwrap(), want, "{}", case["why"]);
    }
    for case in st["invalid"].as_array().unwrap() {
        assert!(parse_protocol_state(s(&case["payload"])).is_err(), "{}", case["why"]);
    }
    for case in v["sim_protocol_topics"].as_array().unwrap() {
        let want = case["parts"].as_array().map(|p| (s(&p[0]).to_string(), s(&p[1]).to_string()));
        assert_eq!(parse_sim_protocol_topic(s(&case["topic"])), want, "{}", case["topic"]);
    }
}

#[test]
fn labels_count_an_emoji_once() {
    let v = vectors();
    for case in v["labels"].as_array().unwrap() {
        assert_eq!(Label::new(s(&case["full"])).to_value()["l16"], case["l16"]);
        let want = Label::new(s(&case["full"])).to_value();
        for k in ["l6", "l8", "l16", "chars", "over6", "over8", "over16"] {
            assert_eq!(want[k], case[k], "{} {k}", case["full"]);
        }
    }
    for case in v["label_overrides"].as_array().unwrap() {
        let l = Label::with_overrides(s(&case["full"]), case["l6"].as_str(), case["l8"].as_str(), case["l16"].as_str());
        for k in ["l6", "l8", "l16"] {
            assert_eq!(l.to_value()[k], case["want"][k], "{k}");
        }
    }
    for case in v["widths"].as_array().unwrap() {
        assert_eq!(width(s(&case["s"])), case["width"].as_u64().unwrap() as usize, "{}", case["why"]);
    }
}

#[test]
fn identities() {
    let v = vectors();
    for pair in v["ports"]["user"].as_array().unwrap() {
        let (n, id) = (pair[0].as_u64().unwrap() as usize, s(&pair[1]));
        assert_eq!(user_port(n), id);
        assert_eq!(parse_port(id), Some(("user", n)));
    }
    for pair in v["ports"]["caster"].as_array().unwrap() {
        let (n, id) = (pair[0].as_u64().unwrap() as usize, s(&pair[1]));
        assert_eq!(caster_port(n), id);
        assert_eq!(parse_port(id), Some(("caster", n)));
    }
    for bad in v["ports"]["invalid"].as_array().unwrap() {
        assert_eq!(parse_port(s(bad)), None, "{bad}");
    }
    for f in v["frames"].as_array().unwrap() {
        assert_eq!(frame_id(s(&f[0]), f[1].as_u64().unwrap() as usize), s(&f[2]));
    }
    for t in v["trunks"].as_array().unwrap() {
        assert_eq!(trunk_id(t[0].as_u64().unwrap() as usize), s(&t[1]));
    }
    for pair in v["slugs"].as_array().unwrap() {
        assert_eq!(slugify(s(&pair[0])), s(&pair[1]));
    }
}

#[test]
fn commands_parse_round_trip_and_refuse_against_their_id() {
    let v = vectors();
    for case in v["commands"]["valid"].as_array().unwrap() {
        let body = case["doc"].to_string();
        let c = parse_command(body.as_bytes()).unwrap_or_else(|r| panic!("{}: {}", case["why"], r.reason));
        assert_eq!(parse_command(c.to_json().as_bytes()).as_ref(), Ok(&c), "{}", case["why"]);
    }
    for case in v["commands"]["invalid"].as_array().unwrap() {
        let body = case["body"].as_str().map(str::to_string).unwrap_or_else(|| case["doc"].to_string());
        let r = parse_command(body.as_bytes()).err().unwrap_or_else(|| panic!("accepted: {}", case["why"]));
        assert_eq!(r.id.as_deref(), case["refused_id"].as_str(), "{}", case["why"]);
    }
    for case in v["results"].as_array().unwrap() {
        let a = case["args"].as_array().unwrap();
        let out = result(a[0].as_str(), a[1].as_str(), s(&a[2]), s(&a[3]), (!a[4].is_null()).then(|| a[4].clone()));
        assert_eq!(serde_json::from_str::<Value>(&out).unwrap(), case["want"]);
    }
}

fn build(builder: &str, a: &[Value]) -> Result<(String, bool), String> {
    let e = |r: Result<(String, bool), apkaudio_contracts::topics::TopicError>| r.map_err(|e| e.to_string());
    match builder {
        "frame_doc_topic" => e(frame_doc_topic(s(&a[0]))),
        "frame_topic" => e(frame_topic(s(&a[0]))),
        "trunk_topic" => e(trunk_topic(s(&a[0]))),
        "connection_topic" => e(connection_topic(s(&a[0]))),
        "crosspoints_topic" => e(crosspoints_topic(s(&a[0]))),
        "booking_topic" => e(booking_topic(s(&a[0]))),
        "site_topic" => e(site_topic(s(&a[0]))),
        "relay_binding_topic" => e(relay_binding_topic(s(&a[0]), a[1].as_u64().unwrap() as usize)),
        "event_topic" => e(event_topic(s(&a[0]))),
        "view_topic" => e(view_topic(s(&a[0]))),
        "view_item_topic" => e(view_item_topic(s(&a[0]), s(&a[1]))),
        "command_topic" => Ok(command_topic()),
        "result_topic" => Ok(result_topic()),
        "sim_frame_topic" => e(sim_frame_topic(s(&a[0]), s(&a[1]))),
        "line_topic" => e(line_topic(s(&a[0]), s(&a[1]))),
        "ack_topic" => e(ack_topic(s(&a[0]), s(&a[1]))),
        "reject_topic" => e(reject_topic(s(&a[0]), s(&a[1]))),
        "protocol_line_topic" => e(protocol_line_topic(s(&a[0]))),
        "protocol_ack_topic" => e(protocol_ack_topic(s(&a[0]))),
        "protocol_state_topic" => e(protocol_state_topic(s(&a[0]))),
        "voice_topic" => e(voice_topic(s(&a[0]), s(&a[1]), a[2].as_u64().unwrap() as u32)),
        other => panic!("no builder {other}"),
    }
}

#[test]
fn topics() {
    let v = vectors();
    for case in v["topics"].as_array().unwrap() {
        let got = build(s(&case["builder"]), case["args"].as_array().unwrap()).unwrap();
        assert_eq!(got, (s(&case["topic"]).to_string(), case["retain"].as_bool().unwrap()), "{}", case["builder"]);
    }
    for case in v["topic_refusals"].as_array().unwrap() {
        assert!(build(s(&case["builder"]), case["args"].as_array().unwrap()).is_err(), "{}", case["why"]);
    }
    assert_eq!(caster_wildcard(), s(&v["wildcards"]["caster"]));
    assert_eq!(sim_wildcard(None).unwrap(), s(&v["wildcards"]["sim"]));
    assert_eq!(sim_wildcard(Some("dante")).unwrap(), s(&v["wildcards"]["sim_dante"]));
    for case in v["sim_topics"].as_array().unwrap() {
        let want = case["parts"].as_array().map(|p| (s(&p[0]).to_string(), s(&p[1]).to_string(), s(&p[2]).to_string()));
        assert_eq!(parse_sim_topic(s(&case["topic"])), want);
    }
}

fn frame_doc_of(w: &Value) -> FrameDoc {
    let map = |k: &str| -> std::collections::BTreeMap<String, Value> { w[k].as_object().unwrap().iter().map(|(k, x)| (k.clone(), x.clone())).collect() };
    let opt = |k: &str| (!w[k].is_null()).then(|| w[k].clone());
    FrameDoc { frame: s(&w["frame"]).to_string(), device: opt("device"), trunks: map("trunks"), connections: map("connections"), crosspoints: opt("crosspoints") }
}

#[test]
fn frame_docs_fold_a_frame_into_one_retained_document() {
    let v = vectors();
    for case in v["frame_docs"]["valid"].as_array().unwrap() {
        let want = frame_doc_of(&case["want"]);
        assert_eq!(parse_frame_doc(s(&case["payload"])).unwrap(), want, "{}", case["why"]);
        assert_eq!(parse_frame_doc(&render_frame_doc(&want)).unwrap(), want, "{}", case["why"]);
    }
    for case in v["frame_docs"]["invalid"].as_array().unwrap() {
        assert!(parse_frame_doc(s(&case["payload"])).is_err(), "{}", case["why"]);
    }
    for case in v["frame_doc_topics"].as_array().unwrap() {
        assert_eq!(parse_frame_doc_topic(s(&case["topic"])), case["frame"].as_str().map(str::to_string), "{}", case["topic"]);
    }
    assert!(FrameDoc { frame: "x".into(), ..Default::default() }.is_empty());
}
