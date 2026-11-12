// Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
// MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
//! Part of `topics.rs` — see that file for the whole picture.

use super::*;

// ------------------------------------------------------------------ parse

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ParsedTopic {
    #[serde(rename = "webrtcSignal")]
    WebrtcSignal { room: String, role: String },
    #[serde(rename = "audioCue")]
    AudioCue { name: String },
    #[serde(rename = "audioEngine")]
    AudioEngine { app: String, leaf: String },
    #[serde(rename = "yoSpeech")]
    YoSpeech { leaf: String },
    #[serde(rename = "yoAlarm")]
    YoAlarm,
    #[serde(rename = "yoStatus")]
    YoStatus,
    #[serde(rename = "yoAnnounce")]
    YoAnnounce,
    #[serde(rename = "yoEcho")]
    YoEcho,
    #[serde(rename = "yoChatInput")]
    YoChatInput { room: String },
    #[serde(rename = "yoVoiceCapture")]
    YoVoiceCapture,
    #[serde(rename = "midi")]
    Midi { device: String, channel: String, control: String },
    #[serde(rename = "timeMark")]
    TimeMark,
    #[serde(rename = "timeHour")]
    TimeHour,
    #[serde(rename = "discovery")]
    Discovery {
        protocol: String,
        #[serde(rename = "deviceId")]
        device_id: String,
    },
    #[serde(rename = "discoveryVerified")]
    DiscoveryVerified {
        protocol: String,
        #[serde(rename = "deviceId")]
        device_id: String,
    },
    #[serde(rename = "discoveryRecord")]
    DiscoveryRecord {
        protocol: String,
        #[serde(rename = "deviceId")]
        device_id: String,
    },
    #[serde(rename = "discoveryEngine")]
    DiscoveryEngine { leaf: String, path: Vec<String> },
    #[serde(rename = "discoveryLifecycle")]
    DiscoveryLifecycle,
    #[serde(rename = "discoveryDeath")]
    DiscoveryDeath,
    #[serde(rename = "gui")]
    Gui { segments: Vec<String> },
    #[serde(rename = "guiState")]
    GuiState {
        #[serde(rename = "stateRoot")]
        state_root: String,
        path: Vec<String>,
    },
    #[serde(rename = "shellSeat")]
    ShellSeat {
        path: Vec<String>,
        leaf: String,
        #[serde(rename = "clientId")]
        client_id: String,
    },
    #[serde(rename = "komandCmd")]
    KomandCmd {
        verb: String,
        #[serde(rename = "deviceClass")]
        device_class: String,
        model: String,
    },
    #[serde(rename = "komandState")]
    KomandState {
        #[serde(rename = "deviceClass")]
        device_class: String,
        model: String,
        capability: String,
    },
    #[serde(rename = "komandMonitor")]
    KomandMonitor { dir: String },
    #[serde(rename = "komandClockArmed")]
    KomandClockArmed,
    #[serde(rename = "agents")]
    Agents { agent: String },
    #[serde(rename = "config")]
    Config { agent: String },
    #[serde(rename = "log")]
    Log { source: String, level: String },
    #[serde(rename = "problems")]
    Problems { source: String, key: String },
    #[serde(rename = "dataMigration")]
    DataMigration { source: String, key: String },
    #[serde(rename = "repositoryManager")]
    RepositoryManager { path: Vec<String> },
    #[serde(rename = "capturePoint")]
    CapturePoint { category: String, key: String, terminal: String },
    #[serde(rename = "capturePointSweep")]
    CapturePointSweep,
    #[serde(rename = "capturePointPoke")]
    CapturePointPoke,
    #[serde(rename = "capture")]
    Capture {
        #[serde(rename = "captureKind")]
        capture_kind: String,
        capture: String,
    },
    #[serde(rename = "protocols")]
    Protocols { proto: String, path: Vec<String> },
    #[serde(rename = "protocolsScanLog")]
    ProtocolsScanLog { proto: String },
    #[serde(rename = "fileBrowser")]
    FileBrowser { key: String },
    #[serde(rename = "guiPanels")]
    GuiPanels { key: String },
    #[serde(rename = "agentCounters")]
    AgentCounters { agent: String },
    #[serde(rename = "bareMetal")]
    BareMetal { target: String, path: Vec<String> },
    #[serde(rename = "bareMetalCommand")]
    BareMetalCommand { target: String, path: Vec<String> },
    #[serde(rename = "bareMetalStream")]
    BareMetalStream { target: String, path: Vec<String>, leaf: String },
    #[serde(rename = "games")]
    Games { game: String, leaf: String },
    #[serde(rename = "gamesBingoDrawn")]
    GamesBingoDrawn,
    #[serde(rename = "cepo")]
    Cepo { unit: String, path: Vec<String> },
    #[serde(rename = "containerManagerEvent")]
    ContainerManagerEvent,
    #[serde(rename = "containerManagerStatus")]
    ContainerManagerStatus,
    #[serde(rename = "containerManagerManager")]
    ContainerManagerManager,
    #[serde(rename = "containerManagerContainer")]
    ContainerManagerContainer { stack: String, name: String },
    #[serde(rename = "containerManagerApp")]
    ContainerManagerApp { stack: String, name: String, app: String },
    #[serde(rename = "containerManagerDockerRequest")]
    ContainerManagerDockerRequest,
    #[serde(rename = "containerManagerDockerReply")]
    ContainerManagerDockerReply { client: String },
    #[serde(rename = "systemChat")]
    SystemChat,
    #[serde(rename = "bareMetalEsp32Heartbeat")]
    BareMetalEsp32Heartbeat { node: String },
    #[serde(rename = "nearestRustAgentHandOff")]
    NearestRustAgentHandOff,
    #[serde(rename = "census")]
    Census { node: String, path: Vec<String> },
    #[serde(rename = "guiDiscovered")]
    GuiDiscovered { path: Vec<String> },
    #[serde(rename = "deviceGroupsState")]
    DeviceGroupsState,
    #[serde(rename = "deviceGroupsCommand")]
    DeviceGroupsCommand,
    #[serde(rename = "hourChimeState")]
    HourChimeState,
    #[serde(rename = "hourChimeStruck")]
    HourChimeStruck,
    #[serde(rename = "captureFiled")]
    CaptureFiled,
    #[serde(rename = "systemWrites")]
    SystemWrites,
    #[serde(rename = "clientBreadcrumb")]
    ClientBreadcrumb,
    #[serde(rename = "benchTwoState")]
    BenchTwoState,
    #[serde(rename = "legacy")]
    Legacy(LegacyTopic),
    #[serde(rename = "unknown")]
    Unknown { raw: String },
}

/// The v40 namespace map (guidelines T7) — same families as topics/legacy.ts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyTopic {
    pub family: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segments: Option<Vec<String>>,
}

impl LegacyTopic {
    pub(super) fn new(family: &str) -> Self {
        LegacyTopic { family: family.to_string(), protocol: None, channel: None, guid: None, segments: None }
    }
}
