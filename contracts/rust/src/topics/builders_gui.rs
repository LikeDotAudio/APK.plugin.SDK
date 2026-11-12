// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Part of `topics.rs` — see that file for the whole picture.

use super::*;

// ------------------------------------------------- builders added 2026-10-30
// The families and shapes the 2026-10-30 architecture audit found consumers
// hand-writing. Each has a TS twin in `topics/builders.ts` and vectors in
// `vectors/topics.json`; the Python binding builds the same ones from the
// generated grammar (`apkaudio_contracts.families`).

/// Any other topic under `Gui/` — a widget's gesture or a status field a panel
/// binds to (`Gui/Chat/Message`, `Gui/Sql/Status/Connected`). Refuses a path
/// that starts at a state root (use [`gui_state`]) or whose second-to-last
/// segment is a seat leaf (use [`gui_seat`]), so the retain class a caller
/// gets never depends on where its path happened to end.
pub fn gui(path: &[&str]) -> Result<String, TopicError> {
    let Some(first) = path.first() else { return Err(err("gui path", "")) };
    for seg in path {
        require_segment("path segment", seg)?;
    }
    if GUI_STATE_ROOTS.contains(first) {
        return Err(err("gui path (a state root: use gui_state)", first));
    }
    if path.len() >= 2 && SHELL_SEAT_LEAVES.contains(&path[path.len() - 2]) {
        return Err(err("gui path (a seat leaf: use gui_seat)", path[path.len() - 2]));
    }
    Ok(format!("{GUI_ROOT}/{}", path.join("/")))
}

/// The bench chat's room-wide line, `Gui/Chat/Message` (YoControl + the shell's Chat).
pub fn gui_chat_message() -> String {
    format!("{GUI_ROOT}/Chat/Message")
}

/// One chat roster seat, `Gui/Chat/Presence/{clientId}` (retained claim).
pub fn gui_chat_presence(client_id: &str) -> Result<String, TopicError> {
    gui_seat(&["Chat"], "Presence", client_id)
}

pub fn gui_chat_presence_wildcard() -> String {
    format!("{GUI_ROOT}/Chat/Presence/+")
}

/// A standing SOS, `Gui/Chat/SOS/{caller}` (retained until stood down).
pub fn gui_chat_sos(caller: &str) -> Result<String, TopicError> {
    gui_seat(&["Chat"], "SOS", caller)
}

pub fn gui_chat_sos_wildcard() -> String {
    format!("{GUI_ROOT}/Chat/SOS/+")
}

/// PLUGIN.SQL's panel status field, `Gui/Sql/Status/{field}`.
pub fn gui_sql_status(field: &str) -> Result<String, TopicError> {
    gui(&["Sql", "Status", field])
}

/// PLUGIN.SQL's capture counters, `Gui/Sql/Capture/{field}`.
pub fn gui_sql_capture(field: &str) -> Result<String, TopicError> {
    gui(&["Sql", "Capture", field])
}

pub fn gui_sql_wildcard() -> String {
    format!("{GUI_ROOT}/Sql/#")
}

/// The leaf a VISA instrument takes a SCPI write on. TS twin `DEVICE_WRITE_LEAF`.
pub const DEVICE_WRITE_LEAF: &str = "Write";

/// A VISA instrument's own subtree, `System/Protocols/visa/Device/{family}/{model}/{dev}`
/// — the prefix every field and the `Write`/`Reply` leaves hang off. Still
/// parses as the legacy `visaDeviceTree` family; it is the live wire until VISA
/// publishes DeviceRecord docs, so it is built here once rather than formatted
/// at every call site.
pub fn visa_device(family: &str, model: &str, dev: &str) -> Result<String, TopicError> {
    protocols("visa", &["Device", family, model, dev])
}

/// One instrument's write leaf.
pub fn visa_write(family: &str, model: &str, dev: &str) -> Result<String, TopicError> {
    Ok(format!("{}/{DEVICE_WRITE_LEAF}", visa_device(family, model, dev)?))
}

/// Every instrument's write leaf — the VISA write daemon's subscription.
pub fn visa_write_wildcard() -> String {
    format!("{ROOT}/System/Protocols/visa/Device/+/+/+/{DEVICE_WRITE_LEAF}")
}

/// PLUGIN.MIDI's retained device tree root, `System/Protocols/midi/Device`
/// (legacy `midiDeviceTree`).
pub fn midi_device() -> String {
    format!("{ROOT}/System/Protocols/midi/Device")
}

/// What PLUGIN.MIDI subscribes to for output, `.../midi/Device/Output/#`.
pub fn midi_output_wildcard() -> String {
    format!("{ROOT}/System/Protocols/midi/Device/Output/#")
}

/// A game's retained state, `Games/{game}/{leaf}`.
pub fn games(game: &str, leaf: &str) -> Result<String, TopicError> {
    require_segment("game", game)?;
    require_segment("leaf", leaf)?;
    Ok(format!("{ROOT}/Games/{game}/{leaf}"))
}

/// The bingo caller's drawn numbers, `Games/Bingo/Drawn` — payload `BingoDrawn`
/// (`src/games-rule.ts`; `{"drawn":[…],"at":<ms>}`, retained, empty = game over).
pub fn games_bingo_drawn() -> String {
    format!("{ROOT}/Games/Bingo/Drawn")
}

pub fn games_wildcard(game: Option<&str>) -> Result<String, TopicError> {
    match game {
        None => Ok(format!("{ROOT}/Games/#")),
        Some(g) => {
            require_segment("game", g)?;
            Ok(format!("{ROOT}/Games/{g}/+"))
        }
    }
}

/// A CEPO Executive Level rack unit, `Cepo/{unit}/{...path}` (YoControl's tool
/// stage: `{unit}/{command}/Set`, `{unit}/Status/Get`). The path may not be empty.
pub fn cepo(unit: &str, path: &[&str]) -> Result<String, TopicError> {
    require_segment("unit", unit)?;
    if path.is_empty() {
        return Err(err("cepo path", ""));
    }
    let mut topic = format!("{ROOT}/Cepo/{unit}");
    for s in path {
        require_segment("path segment", s)?;
        topic.push('/');
        topic.push_str(s);
    }
    Ok(topic)
}

pub fn cepo_wildcard(unit: Option<&str>) -> Result<String, TopicError> {
    match unit {
        None => Ok(format!("{ROOT}/Cepo/#")),
        Some(u) => {
            require_segment("unit", u)?;
            Ok(format!("{ROOT}/Cepo/{u}/#"))
        }
    }
}

fn require_address(what: &'static str, s: &str) -> Result<(), TopicError> {
    if is_address(s) { Ok(()) } else { Err(err(what, s)) }
}

/// DockTor's event stream, the bare `System/ContainerManager` (live).
pub fn container_manager_events() -> String {
    format!("{ROOT}/System/ContainerManager")
}

/// The whole bench in one retained document.
pub fn container_manager_status() -> String {
    format!("{ROOT}/System/ContainerManager/status")
}

/// The publisher itself (retained).
pub fn container_manager_manager() -> String {
    format!("{ROOT}/System/ContainerManager/Manager")
}

/// One container, `containers/{stack}/{name}` (retained; tombstoned empty).
pub fn container_manager_container(stack: &str, name: &str) -> Result<String, TopicError> {
    require_address("stack", stack)?;
    require_address("name", name)?;
    Ok(format!("{ROOT}/System/ContainerManager/containers/{stack}/{name}"))
}

/// One app inside a container, `containers/{stack}/{name}/apps/{app}`.
pub fn container_manager_app(stack: &str, name: &str, app: &str) -> Result<String, TopicError> {
    require_address("app", app)?;
    Ok(format!("{}/apps/{app}", container_manager_container(stack, name)?))
}

pub fn container_manager_containers_wildcard(stack: Option<&str>) -> Result<String, TopicError> {
    match stack {
        None => Ok(format!("{ROOT}/System/ContainerManager/containers/#")),
        Some(s) => {
            require_address("stack", s)?;
            Ok(format!("{ROOT}/System/ContainerManager/containers/{s}/#"))
        }
    }
}

/// The engine's Docker-over-the-bus request (live; DockTor answers).
pub fn container_manager_docker_request() -> String {
    format!("{ROOT}/System/ContainerManager/docker/request")
}

/// Where DockTor answers one requester, `docker/reply/{client}` (live).
pub fn container_manager_docker_reply(client: &str) -> Result<String, TopicError> {
    require_address("client", client)?;
    Ok(format!("{ROOT}/System/ContainerManager/docker/reply/{client}"))
}

pub fn container_manager_docker_reply_wildcard() -> String {
    format!("{ROOT}/System/ContainerManager/docker/reply/+")
}
