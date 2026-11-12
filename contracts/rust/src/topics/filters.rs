// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Part of `topics.rs` — see that file for the whole picture.

/// DECLARED WILDCARD FILTERS — subscription forms that cut across families, so
/// no family's own `#` wildcard spells them. The mirror convention: an agent
/// that copies bus traffic publishes the copy at `APK.audio/{Agent}/monitor/in`
/// and what it sends on at `.../monitor/out` (AUDIO.komand's `Komand/monitor`).
/// `APK.audio/+/monitor/in` is "every agent's mirror" (PLUGIN.SQL's
/// `topic_ignore`). A filter, not a topic: `parse` still refuses it. TS twin:
/// `topics/grammar.ts` DECLARED_FILTERS; vectors: `filters` in topics.json.
pub const DECLARED_FILTERS: [&str; 2] = ["APK.audio/+/monitor/in", "APK.audio/+/monitor/out"];

/// True when `filter` is one of [`DECLARED_FILTERS`], spelled exactly.
pub fn is_declared_filter(filter: &str) -> bool {
    DECLARED_FILTERS.contains(&filter)
}
