// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! The two `config.ini` keys presence reads, and reading them out of a
//! plugin's section.

use std::time::Duration;

use serde_json::Value;

/// The `config.ini` key naming the `{protocol}` levels this plugin writes.
pub const KEY_PROTOCOLS: &str = "presence_protocols";
/// The `config.ini` key: seconds without a sighting before a device is offline.
/// `0` (or absent) means this plugin's protocol announces departures itself
/// and nothing expires on a clock.
pub const KEY_INACTIVE_AFTER: &str = "presence_inactive_after_s";

/// Read `presence_protocols` and `presence_inactive_after_s` out of a plugin's
/// config section. `None` when the plugin declares no protocols — it reports
/// no devices, and gets no session.
pub fn settings_from_config(config: &Value, token: &str) -> Option<(Vec<String>, Option<Duration>)> {
    let section = config
        .as_object()?
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(token))
        .map(|(_, s)| s)?;
    let protocols: Vec<String> = section
        .get(KEY_PROTOCOLS)?
        .as_str()?
        .split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    if protocols.is_empty() {
        return None;
    }
    let after = section
        .get(KEY_INACTIVE_AFTER)
        .and_then(Value::as_str)
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .filter(|d| !d.is_zero());
    Some((protocols, after))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_settings_come_from_the_plugin_section() {
        let config = crate::ini_to_json("[dante]\npresence_protocols = dante\npresence_inactive_after_s = 360\n");
        let (protocols, after) = settings_from_config(&config, "dante").unwrap();
        assert_eq!(protocols, vec!["dante".to_string()]);
        assert_eq!(after, Some(Duration::from_secs(360)));
        let departures = crate::ini_to_json("[printers]\npresence_protocols = printers\npresence_inactive_after_s = 0\n");
        assert_eq!(settings_from_config(&departures, "printers").unwrap().1, None);
        assert!(settings_from_config(&crate::ini_to_json("[osc]\nenabled = true\n"), "osc").is_none());
    }
}
