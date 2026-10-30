// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! WHO THIS AGENT IS TO THE BROKER — the one place credentials are read.
//!
//! The bus was anonymous because it was loopback: `allow_anonymous true` and
//! `listener … 127.0.0.1` were a pair, and only one of them was ever a
//! security control. Binding the broker to the LAN breaks that pair, so every
//! client now has to say who it is.
//!
//! CREDENTIALS COME FROM THE ENVIRONMENT AND NOWHERE ELSE. Not from
//! `config.ini` — those files are published to the bus RETAINED at
//! `APK.audio/System/Config/<protocol>`, so a password in one is a password
//! broadcast to every subscriber, which is the disclosure this module exists
//! to prevent. Not from a committed file either: `Server:Broker:MQTT/passwd`
//! is gitignored for the same reason.
//!
//! This crate may not depend on a client library (see Cargo.toml), so this
//! hands back the pair and the caller applies it — `options.set_credentials(…)`
//! for rumqttc. That split is deliberate: the policy lives here once, the
//! transport stays in the agents.
//!
//! Absent variables mean an anonymous connect, which is correct and is what a
//! loopback bench still wants. A broker with `allow_anonymous false` refuses
//! that connection, and rumqttc reports it as a refused CONNACK rather than a
//! timeout — the two failures look nothing alike in a log, on purpose.

use std::env;

/// Environment variable naming the broker user.
pub const USERNAME_VARIABLE: &str = "APK_MQTT_USERNAME";

/// Environment variable carrying that user's password.
pub const PASSWORD_VARIABLE: &str = "APK_MQTT_PASSWORD";

/// The credentials this process should connect with, if it has any.
///
/// Both variables must be present and non-empty. A username with no password
/// is not a partial credential to be attempted — it is a misconfiguration that
/// would connect anonymously while looking authenticated in the process
/// environment, so it is refused here and reported by [`describe`].
pub fn credentials() -> Option<(String, String)> {
    let username = env::var(USERNAME_VARIABLE).ok()?;
    let password = env::var(PASSWORD_VARIABLE).ok()?;
    if username.trim().is_empty() || password.is_empty() {
        return None;
    }
    Some((username, password))
}

/// One line for the boot log: who this run connects as, never the secret.
///
/// Printed rather than assumed because the failure this replaces is an agent
/// that silently connected anonymously to a broker that happened to still
/// allow it, and was therefore invisible the day the broker stopped.
pub fn describe() -> String {
    describe_from(
        env::var(USERNAME_VARIABLE).ok().as_deref(),
        env::var(PASSWORD_VARIABLE).ok().as_deref(),
    )
}

/// [`describe`] for a given pair, so the rule is testable without touching the
/// process environment.
///
/// EMPTY IS ABSENT. Compose writes `${APK_MQTT_USERNAME:-}` as an empty string
/// when the host never set it, so "present but empty" is the ordinary anonymous
/// bench, not a misconfiguration. Half set means exactly one of the two carries
/// a value — judged as [`credentials`] judges them.
fn describe_from(username: Option<&str>, password: Option<&str>) -> String {
    let user = username.is_some_and(|v| !v.trim().is_empty());
    let pass = password.is_some_and(|v| !v.is_empty());
    match (user, pass) {
        (true, true) => format!("as {}", username.unwrap_or_default()),
        (false, false) => "anonymously".to_string(),
        _ => format!(
            "ANONYMOUSLY — {USERNAME_VARIABLE}/{PASSWORD_VARIABLE} are half set, \
             which is a misconfiguration, not a credential"
        ),
    }
}

#[cfg(test)]
mod tests {
    // Deliberately no test that sets the variables: `std::env::set_var` is
    // process-global and these tests run threaded, so one test's credentials
    // become another's. The behaviour under absent variables is what every
    // caller depends on and is what is asserted.
    #[test]
    fn absent_variables_connect_anonymously() {
        if std::env::var(super::USERNAME_VARIABLE).is_err() {
            assert_eq!(super::credentials(), None);
            assert_eq!(super::describe(), "anonymously");
        }
    }

    #[test]
    fn empty_variables_are_absent_not_half_set() {
        assert_eq!(super::describe_from(Some(""), Some("")), "anonymously");
        assert_eq!(super::describe_from(Some(" "), None), "anonymously");
        assert_eq!(super::describe_from(Some("node"), Some("pw")), "as node");
        assert!(super::describe_from(Some("node"), Some("")).contains("half set"));
        assert!(super::describe_from(None, Some("pw")).contains("half set"));
    }
}
