//! LAN host/join multiplayer: the pre-coaching lobby (main menu, mDNS
//! discovery / manual IP connect) and the lobby protocol that merges each
//! side's finished tactical output before the Game phase starts.
use crate::game_handoff::{CoachedTeam, MatchHandoff, TeamSide};
use crate::interpretation::TacticalResult;
use crate::phase::AppPhase;
use crate::session::CoachingSession;
use crate::{EnterGame, SetCoachingActive};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use crossbeam_channel::{unbounded, Receiver, Sender};
use futures_util::StreamExt;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use serde_json::{json, Value};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

pub const DEFAULT_API_PORT: u16 = 8787;
const SERVICE_TYPE: &str = "_tacticlab._tcp.local.";
const SERVICE_INSTANCE: &str = "tacticlab-host";

/// Which role this process is playing in the current match. `Solo` is the
/// default and behaves exactly like the pre-multiplayer single-player app:
/// no lobby networking, no game streaming, full local simulation.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NetworkRole {
    #[default]
    Solo,
    Host,
    Joiner,
}

impl NetworkRole {
    pub fn is_host(&self) -> bool {
        !matches!(self, NetworkRole::Joiner)
    }

    pub fn is_joiner(&self) -> bool {
        matches!(self, NetworkRole::Joiner)
    }

    pub fn is_networked(&self) -> bool {
        !matches!(self, NetworkRole::Solo)
    }

    /// Fixed assignment agreed for this app: host always coaches red,
    /// joiner always coaches yellow.
    pub fn coached_side(&self) -> TeamSide {
        match self {
            NetworkRole::Joiner => TeamSide::Yellow,
            NetworkRole::Host | NetworkRole::Solo => TeamSide::Red,
        }
    }
}

/// Run condition: true for the joining machine only.
pub fn is_spectator(role: Option<Res<NetworkRole>>) -> bool {
    role.map(|role| role.is_joiner()).unwrap_or(false)
}

/// Run condition: true for everyone except the joining machine (host or solo).
pub fn is_not_spectator(role: Option<Res<NetworkRole>>) -> bool {
    !is_spectator(role)
}

/// The host's `ip:port` for its Node service. For `Host`/`Solo` this is
/// always local; for `Joiner` it is set once the lobby connect succeeds.
#[derive(Resource, Clone, Debug)]
pub struct NetworkEndpoint {
    pub host_addr: String,
}

impl Default for NetworkEndpoint {
    fn default() -> Self {
        Self {
            host_addr: format!("127.0.0.1:{DEFAULT_API_PORT}"),
        }
    }
}

impl NetworkEndpoint {
    pub fn host_only(&self) -> &str {
        self.host_addr.split(':').next().unwrap_or(&self.host_addr)
    }
}

#[derive(Event, Clone, Copy, Debug)]
pub struct MatchReady;

/// What the player-creation phase produced, captured from `ContinueToCoaching`
/// (which the app previously discarded). Needed so the coaching side knows
/// which drawings belong to this session and can ship them to the host.
#[derive(Resource, Clone, Debug, Default)]
pub struct CreationArtifacts {
    pub session_id: Option<String>,
    pub directory: Option<std::path::PathBuf>,
}

/// One player's complete contribution to a match: the tactical output plus
/// everything needed to reproduce and learn from the session later.
#[derive(Clone, Debug)]
pub struct MatchEntry {
    pub role: NetworkRole,
    pub match_id: String,
    pub tactical_output: Value,
    pub coaching_session_id: String,
    pub creation_session_id: Option<String>,
    pub creation_dir: Option<std::path::PathBuf>,
    pub session_json_path: Option<std::path::PathBuf>,
}

impl MatchEntry {
    /// Reads the on-disk artifacts and builds the upload payload. All file IO
    /// happens on the worker thread, never in a Bevy system.
    fn read_bundle(&self, role_label: &str, team_label: &str) -> Result<Value, String> {
        use base64::Engine;

        let session = match &self.session_json_path {
            Some(path) => std::fs::read(path)
                .map_err(|error| format!("Cannot read {}: {error}", path.display()))
                .and_then(|bytes| {
                    serde_json::from_slice::<Value>(&bytes)
                        .map_err(|error| format!("{} is not valid JSON: {error}", path.display()))
                })?,
            None => return Err("No saved session.json to upload.".into()),
        };

        let encode = |name: &str| -> Option<String> {
            let path = self.creation_dir.as_ref()?.join(name);
            let bytes = std::fs::read(path).ok()?;
            Some(base64::engine::general_purpose::STANDARD.encode(bytes))
        };
        let manifest = self
            .creation_dir
            .as_ref()
            .and_then(|dir| std::fs::read(dir.join("manifest.json")).ok())
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());

        let mut bundle = json!({
            "matchId": self.match_id,
            "role": role_label,
            "teamId": team_label,
            "coachingSessionId": self.coaching_session_id,
            "session": session,
            "tacticalOutput": self.tactical_output,
        });
        if let Some(id) = &self.creation_session_id {
            bundle["creationSessionId"] = json!(id);
        }
        if let Some(manifest) = manifest {
            bundle["manifest"] = manifest;
        }
        let mut drawings = json!({});
        if let Some(data) = encode("appearance.png") {
            drawings["appearance"] = json!(data);
        }
        if let Some(data) = encode("superpower.png") {
            drawings["superpower"] = json!(data);
        }
        if drawings.as_object().is_some_and(|map| !map.is_empty()) {
            bundle["drawings"] = drawings;
        }
        Ok(bundle)
    }
}

#[derive(Resource, Default)]
pub struct LobbyUiState {
    pub screen: LobbyScreen,
    pub manual_addr: String,
    pub status: Option<String>,
    pub error: Option<String>,
    pub discovered: Vec<DiscoveredHost>,
    pub connecting: bool,
    pub match_id: Option<String>,
    pub wait: WaitStage,
    pub my_tactic_summary: Option<String>,
    pub opponent_ready: bool,
}

/// How far this machine has got through handing its match entry to the host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WaitStage {
    #[default]
    Idle,
    /// Bundle is being written to the host.
    Uploading,
    /// Host has our artifacts and our tactic; waiting on the other player.
    Submitted,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LobbyScreen {
    #[default]
    MainMenu,
    Hosting,
    Joining,
    /// Microphone, picture, and where the local service is. See `menu::settings_ui`.
    Settings,
    /// What the app believes, and the shortcuts past it. See `menu::admin_ui`.
    Admin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredHost {
    pub label: String,
    pub addr: String,
}

/// Everything the lobby worker threads report back to the Bevy systems.
#[derive(Debug, Clone)]
pub enum LobbyEvent {
    Connected {
        host_addr: String,
        match_id: Option<String>,
    },
    ConnectFailed(String),
    Status(LobbyStatus),
    /// This machine's artifact bundle is safely on the host's disk.
    ArtifactsStored,
    MatchStart {
        match_id: String,
        red: Value,
        yellow: Value,
    },
    Error(String),
}

/// Mirrors the server's `statusPayload()`.
#[derive(Debug, Clone, Default)]
pub struct LobbyStatus {
    pub match_id: String,
    pub host_connected: bool,
    pub joiner_connected: bool,
    pub host_ready: bool,
    pub joiner_ready: bool,
    pub host_artifacts: bool,
    pub joiner_artifacts: bool,
}

impl LobbyStatus {
    fn from_json(value: &Value) -> Self {
        let flag = |key: &str| value[key].as_bool().unwrap_or(false);
        Self {
            match_id: value["matchId"].as_str().unwrap_or_default().to_owned(),
            host_connected: flag("hostConnected"),
            joiner_connected: flag("joinerConnected"),
            host_ready: flag("hostReady"),
            joiner_ready: flag("joinerReady"),
            host_artifacts: flag("hostArtifacts"),
            joiner_artifacts: flag("joinerArtifacts"),
        }
    }

    /// Ready state of the side this process is NOT playing.
    pub fn opponent_ready(&self, role: NetworkRole) -> bool {
        if role.is_joiner() {
            self.host_ready
        } else {
            self.joiner_ready
        }
    }
}

/// Background networking for the lobby: transient blocking HTTP calls for
/// join/ready, plus one persistent WebSocket listener for pushed
/// status/start events (same shape as `SpeechRuntime`'s worker thread).
#[derive(Resource)]
pub struct LobbyRuntime {
    events: Receiver<LobbyEvent>,
    sender: Sender<LobbyEvent>,
    ws_thread_started: bool,
}

impl Default for LobbyRuntime {
    fn default() -> Self {
        let (sender, events) = unbounded();
        Self {
            events,
            sender,
            ws_thread_started: false,
        }
    }
}

impl LobbyRuntime {
    /// Joiner: POST /api/lobby/join to the given address, reporting success/failure.
    pub fn join(&self, host_addr: String) {
        let sender = self.sender.clone();
        let addr = host_addr.clone();
        std::thread::spawn(move || {
            match post_json(&addr, "/api/lobby/join", &json!({ "role": "joiner" })) {
                Ok(status) => {
                    let _ = sender.send(LobbyEvent::Connected {
                        host_addr: addr,
                        match_id: status["matchId"].as_str().map(str::to_owned),
                    });
                }
                Err(message) => {
                    let _ = sender.send(LobbyEvent::ConnectFailed(message));
                }
            }
        });
    }

    /// Host: POST /api/lobby/join for itself, against its own local server.
    pub fn announce_host(&self, host_addr: String) {
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            match post_json(&host_addr, "/api/lobby/join", &json!({ "role": "host" })) {
                Ok(status) => {
                    let _ = sender.send(LobbyEvent::Connected {
                        host_addr,
                        match_id: status["matchId"].as_str().map(str::to_owned),
                    });
                }
                Err(message) => {
                    let _ = sender.send(LobbyEvent::ConnectFailed(message));
                }
            }
        });
    }

    /// Uploads this machine's full artifact bundle and, only once the host has
    /// it on disk, posts the tactical output that makes this side ready. The
    /// server refuses `ready` without artifacts, so this ordering is what
    /// guarantees no player's data is lost before a match begins.
    pub fn submit_match_entry(&self, host_addr: String, entry: MatchEntry) {
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let role_label = if entry.role.is_joiner() {
                "joiner"
            } else {
                "host"
            };
            let team_label = match entry.role.coached_side() {
                TeamSide::Red => "red",
                TeamSide::Yellow => "yellow",
            };

            let bundle = match entry.read_bundle(role_label, team_label) {
                Ok(bundle) => bundle,
                Err(message) => {
                    let _ = sender.send(LobbyEvent::Error(message));
                    return;
                }
            };
            if let Err(message) = post_json(&host_addr, "/api/lobby/artifacts", &bundle) {
                let _ = sender.send(LobbyEvent::Error(format!("Artifact upload failed: {message}")));
                return;
            }
            let _ = sender.send(LobbyEvent::ArtifactsStored);

            let body = json!({
                "role": role_label,
                "teamId": team_label,
                "tacticalOutput": entry.tactical_output,
            });
            if let Err(message) = post_json(&host_addr, "/api/lobby/ready", &body) {
                let _ = sender.send(LobbyEvent::Error(message));
            }
        });
    }

    /// Starts the persistent WS listener once we know the host address.
    /// Safe to call multiple times; only the first call spawns a thread.
    pub fn ensure_listening(&mut self, host_addr: String) {
        if self.ws_thread_started {
            return;
        }
        self.ws_thread_started = true;
        let sender = self.sender.clone();
        std::thread::Builder::new()
            .name("tactic-lab-lobby-ws".into())
            .spawn(move || lobby_ws_worker(host_addr, sender))
            .expect("lobby ws thread");
    }

    pub fn poll(&self) -> Vec<LobbyEvent> {
        self.events.try_iter().collect()
    }
}

fn post_json(host_addr: &str, path: &str, body: &Value) -> Result<Value, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .post(format!("http://{host_addr}{path}"))
        .json(body)
        .send()
        .map_err(|error| {
            format!("Cannot reach {host_addr}{path}: {error}. Check the address and that the other machine's server is running.")
        })?;
    let status = response.status();
    let text = response.text().map_err(|error| error.to_string())?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {text}"));
    }
    serde_json::from_str(&text).map_err(|error| error.to_string())
}

fn lobby_ws_worker(host_addr: String, sender: Sender<LobbyEvent>) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = sender.send(LobbyEvent::Error(format!("Lobby runtime failed: {error}")));
            return;
        }
    };
    runtime.block_on(async move {
        loop {
            let url = format!("ws://{host_addr}/api/lobby");
            match tokio_tungstenite::connect_async(&url).await {
                Ok((socket, _)) => {
                    let (_write, mut read) = socket.split();
                    while let Some(Ok(message)) = read.next().await {
                        let Message::Text(text) = message else {
                            continue;
                        };
                        let Ok(value) = serde_json::from_str::<Value>(&text) else {
                            continue;
                        };
                        match value["type"].as_str() {
                            Some("status") => {
                                let _ =
                                    sender.send(LobbyEvent::Status(LobbyStatus::from_json(&value)));
                            }
                            Some("start") => {
                                let _ = sender.send(LobbyEvent::MatchStart {
                                    match_id: value["matchId"]
                                        .as_str()
                                        .unwrap_or_default()
                                        .to_owned(),
                                    red: value["red"].clone(),
                                    yellow: value["yellow"].clone(),
                                });
                            }
                            _ => {}
                        }
                    }
                }
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(750)).await;
                }
            }
        }
    });
}

/// mDNS advertise/browse. Best-effort: failures here never block manual
/// IP entry, which is the reliable fallback path for a live demo.
#[derive(Resource, Default)]
pub struct MdnsState {
    daemon: Option<ServiceDaemon>,
    browse: Option<mdns_sd::Receiver<ServiceEvent>>,
}

impl MdnsState {
    fn daemon(&mut self) -> Option<&ServiceDaemon> {
        if self.daemon.is_none() {
            self.daemon = ServiceDaemon::new().ok();
        }
        self.daemon.as_ref()
    }

    pub fn advertise(&mut self, api_port: u16) {
        let Some(daemon) = self.daemon() else { return };
        let Ok(ip) = local_ip_address::local_ip() else {
            return;
        };
        let host_name = format!("{SERVICE_INSTANCE}.local.");
        if let Ok(info) = ServiceInfo::new(
            SERVICE_TYPE,
            SERVICE_INSTANCE,
            &host_name,
            ip.to_string(),
            api_port,
            None,
        ) {
            let _ = daemon.register(info);
        }
    }

    pub fn start_browsing(&mut self) {
        if self.browse.is_some() {
            return;
        }
        if let Some(daemon) = self.daemon() {
            self.browse = daemon.browse(SERVICE_TYPE).ok();
        }
    }

    pub fn poll_discovered(&self) -> Vec<DiscoveredHost> {
        let mut found = Vec::new();
        let Some(browse) = &self.browse else {
            return found;
        };
        while let Ok(event) = browse.try_recv() {
            if let ServiceEvent::ServiceResolved(resolved) = event {
                if let Some(address) = resolved
                    .addresses
                    .iter()
                    .find(|address| address.is_ipv4() && !address.is_loopback())
                {
                    found.push(DiscoveredHost {
                        label: resolved.fullname.clone(),
                        addr: format!("{}:{}", address.to_ip_addr(), resolved.port),
                    });
                }
            }
        }
        found
    }
}

pub struct LobbyPlugin;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NetworkRole>()
            .init_resource::<NetworkEndpoint>()
            .init_resource::<LobbyUiState>()
            .init_resource::<LobbyRuntime>()
            .init_resource::<MdnsState>()
            .init_resource::<CreationArtifacts>()
            .init_resource::<PendingMatchEntry>()
            .add_event::<MatchReady>()
            .add_systems(
                Update,
                lobby_menu_ui
                    .run_if(in_state(AppPhase::Lobby))
                    .run_if(|ui_state: Res<LobbyUiState>| {
                        // Settings and Admin draw their own central panel; two in one frame
                        // would stack.
                        !matches!(ui_state.screen, LobbyScreen::Settings | LobbyScreen::Admin)
                    }),
            )
            .add_systems(
                Update,
                (crate::menu::settings_ui, crate::menu::admin_ui).run_if(in_state(AppPhase::Lobby)),
            )
            .add_systems(
                Update,
                poll_mdns_discoveries.run_if(in_state(AppPhase::Lobby)),
            )
            .add_systems(
                Update,
                waiting_screen_ui.run_if(in_state(AppPhase::Waiting)),
            )
            .add_systems(Update, (receive_lobby_events, handle_match_ready));
    }
}

fn lobby_menu_ui(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<LobbyUiState>,
    mut role: ResMut<NetworkRole>,
    mut endpoint: ResMut<NetworkEndpoint>,
    runtime: Res<LobbyRuntime>,
    mut mdns: ResMut<MdnsState>,
    mut next_phase: ResMut<NextState<AppPhase>>,
) {
    let ctx = contexts.ctx_mut();
    // No panel fill: the jungle standing behind this *is* the backdrop, and a flat colour in
    // front of it is a worse picture than the one it hides. A gradient wash goes under the
    // column instead, so cloth text stays readable over sunlit turf without covering the
    // stadium. See `theme::scrim`.
    egui::CentralPanel::default()
        .frame(egui::Frame::none())
        .show(ctx, |ui| {
            let screen = ui.max_rect();
            let washed = egui::Rect::from_min_size(
                screen.left_top(),
                egui::vec2(screen.width() * crate::theme::SCRIM_FRACTION, screen.height()),
            );
            crate::theme::scrim(ui.painter(), washed, 225, 0);

            // Laid out from the left, as the reference does, rather than centred: the menu is a
            // column down one side and the scenery has the rest of the frame.
            let column = egui::Rect::from_min_size(
                screen.left_top() + egui::vec2(56.0, 64.0),
                egui::vec2(crate::theme::SLAB_MAX_WIDTH, screen.height() - 96.0),
            );
            ui.allocate_ui_at_rect(column, |ui| {
        {
            crate::theme::title(
                ui,
                "Canopy Clash",
                Some("Draw a player. Coach a play. Watch it happen."),
            );

            match ui_state.screen {
                LobbyScreen::MainMenu => {
                    ui.set_max_width(crate::theme::SLAB_MAX_WIDTH);
                    // Solo first, and marked as the primary route: it is the whole flow on one
                    // machine and needs no second player, so it is what most people want.
                    if primary_button(ui, "Play Solo") {
                        *role = NetworkRole::Solo;
                        next_phase.set(AppPhase::Creation);
                    }
                    ui.add_space(8.0);
                    if big_button(ui, "Host Match") {
                        *role = NetworkRole::Host;
                        *endpoint = NetworkEndpoint::default();
                        ui_state.screen = LobbyScreen::Hosting;
                        ui_state.status = Some("Announcing on the LAN…".into());
                        mdns.advertise(DEFAULT_API_PORT);
                        runtime.announce_host(endpoint.host_addr.clone());
                    }
                    ui.add_space(8.0);
                    if big_button(ui, "Join Match") {
                        *role = NetworkRole::Joiner;
                        ui_state.screen = LobbyScreen::Joining;
                        ui_state.discovered.clear();
                        mdns.start_browsing();
                    }
                    ui.add_space(20.0);
                    if big_button(ui, "Settings") {
                        ui_state.screen = LobbyScreen::Settings;
                    }
                    ui.add_space(8.0);
                    if big_button(ui, "Admin") {
                        ui_state.screen = LobbyScreen::Admin;
                    }
                }
                // Drawn by `menu.rs`, which owns everything on them.
                LobbyScreen::Settings | LobbyScreen::Admin => {}
                LobbyScreen::Hosting => {
                    ui.label(format!(
                        "Hosting on {} — share this address with the other player.",
                        endpoint.host_addr
                    ));
                    if let Ok(ip) = local_ip_address::local_ip() {
                        ui.label(format!("LAN address: {ip}:{DEFAULT_API_PORT}"));
                    }
                    ui.add_space(12.0);
                    ui.spinner();
                    ui.label(
                        ui_state
                            .status
                            .clone()
                            .unwrap_or_else(|| "Waiting for a player to join…".into()),
                    );
                    if let Some(error) = &ui_state.error {
                        ui.colored_label(egui::Color32::from_rgb(220, 100, 100), error);
                    }
                    ui.add_space(16.0);
                    if ui.button("Back").clicked() {
                        ui_state.screen = LobbyScreen::MainMenu;
                    }
                }
                LobbyScreen::Joining => {
                    ui.set_max_width(360.0);
                    if !ui_state.discovered.is_empty() {
                        ui.label("Discovered on this network:");
                        for host in ui_state.discovered.clone() {
                            if ui.button(format!("{}  ({})", host.label, host.addr)).clicked() {
                                ui_state.manual_addr = host.addr;
                            }
                        }
                        ui.add_space(8.0);
                    }
                    ui.label("Host address (ip:port):");
                    ui.text_edit_singleline(&mut ui_state.manual_addr);
                    ui.add_space(8.0);
                    let connecting = ui_state.connecting;
                    if ui
                        .add_enabled(!connecting, egui::Button::new("Connect"))
                        .clicked()
                        && !ui_state.manual_addr.trim().is_empty()
                    {
                        ui_state.connecting = true;
                        ui_state.error = None;
                        runtime.join(ui_state.manual_addr.trim().to_owned());
                    }
                    if connecting {
                        ui.spinner();
                    }
                    if let Some(error) = &ui_state.error {
                        ui.colored_label(egui::Color32::from_rgb(220, 100, 100), error);
                    }
                    ui.add_space(16.0);
                    if ui.button("Back").clicked() {
                        ui_state.screen = LobbyScreen::MainMenu;
                    }
                }
            }
        }
            });
        });
}

fn poll_mdns_discoveries(mdns: Res<MdnsState>, mut ui_state: ResMut<LobbyUiState>) {
    for host in mdns.poll_discovered() {
        if !ui_state.discovered.iter().any(|existing| existing.addr == host.addr) {
            ui_state.discovered.push(host);
        }
    }
}

/// Runs in every phase: the match-start push can arrive at any point after
/// the lobby connects (typically during Waiting, once both sides finish).
#[allow(clippy::too_many_arguments)]
fn receive_lobby_events(
    mut commands: Commands,
    mut runtime: ResMut<LobbyRuntime>,
    mut ui_state: ResMut<LobbyUiState>,
    role: Res<NetworkRole>,
    mut endpoint: ResMut<NetworkEndpoint>,
    mut set_active: EventWriter<SetCoachingActive>,
    mut next_phase: ResMut<NextState<AppPhase>>,
    phase: Res<State<AppPhase>>,
) {
    for event in runtime.poll() {
        match event {
            LobbyEvent::Connected {
                host_addr,
                match_id,
            } => {
                ui_state.connecting = false;
                ui_state.error = None;
                ui_state.match_id = match_id;
                endpoint.host_addr = host_addr.clone();
                runtime.ensure_listening(host_addr.clone());
                if *role == NetworkRole::Joiner {
                    redirect_env_to_host(&host_addr);
                    next_phase.set(AppPhase::Creation);
                }
            }
            LobbyEvent::ConnectFailed(message) => {
                ui_state.connecting = false;
                ui_state.error = Some(message);
            }
            LobbyEvent::Status(status) => {
                if !status.match_id.is_empty() {
                    ui_state.match_id = Some(status.match_id.clone());
                }
                // Both roles need this: it drives the waiting screen's
                // "opponent still coaching" line, not just the host's lobby.
                ui_state.opponent_ready = status.opponent_ready(*role);
                if *role == NetworkRole::Host {
                    ui_state.status = Some(if status.joiner_connected {
                        "Player connected!".into()
                    } else {
                        "Waiting for a player to join…".into()
                    });
                    if status.host_connected
                        && status.joiner_connected
                        && *phase.get() == AppPhase::Lobby
                    {
                        next_phase.set(AppPhase::Creation);
                    }
                }
            }
            LobbyEvent::ArtifactsStored => {
                ui_state.wait = WaitStage::Submitted;
                ui_state.error = None;
            }
            LobbyEvent::MatchStart {
                match_id,
                red,
                yellow,
            } => match build_match_handoff(&match_id, &red, &yellow) {
                Ok(handoff) => {
                    commands.insert_resource(handoff.team_tactics());
                    commands.insert_resource(handoff);
                    // Via the event, so `update_lifecycle`'s equality guard
                    // can't swallow the despawn.
                    set_active.send(SetCoachingActive(false));
                    next_phase.set(AppPhase::Game);
                }
                Err(error) => {
                    ui_state.error = Some(format!("Cannot start match: {error:#}"));
                }
            },
            LobbyEvent::Error(message) => {
                ui_state.error = Some(message);
            }
        }
    }
}

/// Consumes the "ready to enter the game" signal from the results screen (the
/// "Next"/"Continue anyway" button). Solo enters the game directly, exactly as
/// before multiplayer existed. Networked play uploads this machine's artifact
/// bundle and tactic, then waits on the `Waiting` screen for the host's merged
/// `MatchStart`.
#[allow(clippy::too_many_arguments)]
fn handle_match_ready(
    mut events: EventReader<MatchReady>,
    role: Res<NetworkRole>,
    endpoint: Res<NetworkEndpoint>,
    runtime: Res<LobbyRuntime>,
    mut ui_state: ResMut<LobbyUiState>,
    result: Res<TacticalResult>,
    session: Res<CoachingSession>,
    creation: Res<CreationArtifacts>,
    mut pending: ResMut<PendingMatchEntry>,
    mut enter_game: EventWriter<EnterGame>,
    mut set_active: EventWriter<SetCoachingActive>,
    mut next_phase: ResMut<NextState<AppPhase>>,
) {
    if events.read().next().is_none() {
        return;
    }
    if !role.is_networked() {
        enter_game.send(EnterGame);
        return;
    }
    let Some(output) = result.output.clone() else {
        return;
    };

    ui_state.my_tactic_summary = CoachedTeam::from_output_for_team(&output, role.coached_side())
        .ok()
        .map(|team| team.summary_line());

    let entry = MatchEntry {
        role: *role,
        match_id: ui_state.match_id.clone().unwrap_or_default(),
        tactical_output: output,
        coaching_session_id: session.session.id.clone(),
        creation_session_id: creation.session_id.clone(),
        creation_dir: creation.directory.clone(),
        session_json_path: result.session_path.clone(),
    };
    pending.0 = Some(entry.clone());

    ui_state.wait = WaitStage::Uploading;
    ui_state.error = None;
    set_active.send(SetCoachingActive(false));
    next_phase.set(AppPhase::Waiting);
    runtime.submit_match_entry(endpoint.host_addr.clone(), entry);
}

/// Keeps the last submitted entry so the waiting screen can retry an upload
/// without forcing the user back through coaching.
#[derive(Resource, Default)]
pub struct PendingMatchEntry(pub Option<MatchEntry>);

/// The screen that used to be a black void: shows upload progress, both
/// players' readiness, and — critically — any error that would otherwise be
/// swallowed while no other UI is running.
#[allow(clippy::too_many_arguments)]
fn waiting_screen_ui(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<LobbyUiState>,
    role: Res<NetworkRole>,
    endpoint: Res<NetworkEndpoint>,
    runtime: Res<LobbyRuntime>,
    pending: Res<PendingMatchEntry>,
    mut set_active: EventWriter<SetCoachingActive>,
    mut next_phase: ResMut<NextState<AppPhase>>,
) {
    let ctx = contexts.ctx_mut();
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(64.0);
        ui.vertical_centered(|ui| {
            ui.set_max_width(420.0);
            ui.heading("Waiting for the other coach");
            ui.add_space(12.0);

            if let Some(summary) = &ui_state.my_tactic_summary {
                ui.label(summary.clone());
                ui.add_space(12.0);
            }

            let uploaded = ui_state.wait == WaitStage::Submitted;
            ui.label(if uploaded {
                "You .............. Ready ✓"
            } else {
                "You .............. Uploading your session…"
            });
            ui.label(if ui_state.opponent_ready {
                "Opponent ......... Ready ✓"
            } else {
                "Opponent ......... still coaching…"
            });

            ui.add_space(16.0);
            if ui_state.error.is_none() {
                ui.spinner();
            }

            if let Some(error) = ui_state.error.clone() {
                ui.colored_label(egui::Color32::from_rgb(220, 100, 100), error);
                ui.add_space(12.0);
                if ui.button("Retry upload").clicked() {
                    if let Some(entry) = pending.0.clone() {
                        ui_state.error = None;
                        ui_state.wait = WaitStage::Uploading;
                        runtime.submit_match_entry(endpoint.host_addr.clone(), entry);
                    }
                }
                // Only offered on failure: going back after a successful
                // submit would desync from the ready state the host recorded.
                if ui.button("Back to coaching").clicked() {
                    ui_state.wait = WaitStage::Idle;
                    ui_state.error = None;
                    set_active.send(SetCoachingActive(true));
                    next_phase.set(AppPhase::Coaching);
                }
            }

            ui.add_space(16.0);
            if let Some(match_id) = &ui_state.match_id {
                ui.label(
                    egui::RichText::new(format!(
                        "Match {}  ·  you are {}",
                        match_id.chars().take(14).collect::<String>(),
                        if role.is_joiner() { "yellow" } else { "red" }
                    ))
                    .small(),
                );
            }
        });
    });
}

fn build_match_handoff(match_id: &str, red: &Value, yellow: &Value) -> anyhow::Result<MatchHandoff> {
    let red_team = CoachedTeam::from_output_for_team(red, TeamSide::Red)?;
    let yellow_team = CoachedTeam::from_output_for_team(yellow, TeamSide::Yellow)?;
    Ok(MatchHandoff {
        match_id: match_id.to_owned(),
        red: red_team,
        yellow: yellow_team,
    })
}

fn redirect_env_to_host(host_addr: &str) {
    std::env::set_var("TACTIC_LAB_API_URL", format!("http://{host_addr}"));
    std::env::set_var(
        "TACTIC_LAB_WS_URL",
        format!("ws://{host_addr}/api/transcribe"),
    );
}

/// One row of the menu.
///
/// A slab rather than a rectangle: see `theme`. `primary` marks the entry most people want, which
/// is the only reason to make one row look different from another.
fn big_button(ui: &mut egui::Ui, label: &str) -> bool {
    crate::theme::slab(ui, label, crate::theme::Tone::Plain, false).clicked()
}

/// The menu row that is probably what you came for.
fn primary_button(ui: &mut egui::Ui, label: &str) -> bool {
    crate::theme::slab(ui, label, crate::theme::Tone::Primary, false).clicked()
}

#[cfg(test)]
mod waiting_phase_tests {
    //! Guards the black screen: after submitting, a networked player must land
    //! on a phase that actually renders something, and a failed merge must
    //! leave a visible error instead of a silent void.
    use super::*;
    use crate::interpretation::InterpretationState;

    fn app_with_ready_result(role: NetworkRole) -> App {
        let mut app = App::new();
        let session = CoachingSession::default();
        let mut result = TacticalResult::default();
        result.state = InterpretationState::Ready;
        result.output = Some(json!({
            "schemaVersion": "2.0", "taxonomyVersion": "tactics-v2",
            "session": { "id": session.session.id },
            "rlSelection": {
                "schemaVersion": "2.0", "taxonomyVersion": "tactics-v2",
                "sessionId": session.session.id, "teamId": "red",
                "primaryTactic": "highpress", "downstreamValue": "highpress",
                "playerOverrides": []
            }
        }));
        app.add_plugins(MinimalPlugins)
            .init_state::<AppPhase>()
            .insert_resource(role)
            .insert_resource(session)
            .insert_resource(result)
            .init_resource::<NetworkEndpoint>()
            .init_resource::<LobbyUiState>()
            .init_resource::<LobbyRuntime>()
            .init_resource::<CreationArtifacts>()
            .init_resource::<PendingMatchEntry>()
            .add_event::<MatchReady>()
            .add_event::<EnterGame>()
            .add_event::<SetCoachingActive>()
            .add_systems(Update, handle_match_ready);
        app
    }

    #[test]
    fn submitting_a_networked_tactic_enters_the_waiting_phase() {
        let mut app = app_with_ready_result(NetworkRole::Host);
        app.world.send_event(MatchReady);
        app.update();
        app.update();

        assert_eq!(
            *app.world.resource::<State<AppPhase>>().get(),
            AppPhase::Waiting,
            "a networked submit must land on a phase that renders a waiting screen"
        );
        assert_eq!(app.world.resource::<LobbyUiState>().wait, WaitStage::Uploading);
        // Retryable without forcing the user back through coaching.
        assert!(app.world.resource::<PendingMatchEntry>().0.is_some());
    }

    #[test]
    fn solo_still_enters_the_game_directly() {
        let mut app = app_with_ready_result(NetworkRole::Solo);
        app.world.send_event(MatchReady);
        app.update();

        assert!(!app.world.resource::<Events<EnterGame>>().is_empty());
        assert_eq!(
            *app.world.resource::<State<AppPhase>>().get(),
            AppPhase::Lobby,
            "solo must not use the networked waiting path"
        );
    }

    #[test]
    fn a_failed_merge_surfaces_an_error_instead_of_a_silent_black_screen() {
        // Yellow payload is malformed, so the merge must fail.
        let error = build_match_handoff(
            "match-test",
            &json!({
                "schemaVersion": "2.0", "taxonomyVersion": "tactics-v2",
                "session": { "id": "host-session" },
                "rlSelection": {
                    "schemaVersion": "2.0", "taxonomyVersion": "tactics-v2",
                    "sessionId": "host-session", "teamId": "red",
                    "primaryTactic": "highpress", "downstreamValue": "highpress",
                    "playerOverrides": []
                }
            }),
            &json!({ "session": { "id": "joiner-session" } }),
        )
        .unwrap_err();
        assert!(!error.to_string().is_empty());
    }
}

#[cfg(test)]
mod lan_simulation_tests {
    //! End-to-end simulation of a real host machine and a real joiner
    //! machine talking to a real Node lobby server over real sockets: an
    //! actual `server/index.ts` process is spawned, and two independent
    //! `LobbyRuntime`s (the exact production networking code the native app
    //! uses) each open real HTTP/WebSocket connections to it — the same
    //! code path as a genuine LAN host/join, just addressed at 127.0.0.1
    //! instead of a routed wifi address.
    use super::*;
    use std::path::PathBuf;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    struct ServerGuard(Child);
    impl Drop for ServerGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn repo_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .expect("repo root")
    }

    fn spawn_server(port: u16, output_dir: &std::path::Path) -> ServerGuard {
        // Run tsx's CLI directly under `node`, rather than going through `npx`.
        //
        // `npx` is a shell script on Unix and a `.cmd` batch file on Windows, so `Command::new`
        // cannot even find it there (no PATHEXT resolution) — these three tests could never run
        // on a Windows machine. Naming `npx.cmd` finds it but is worse: each spawn becomes a
        // three-process chain (npx-cli -> the `.bin/tsx` shim -> the real server), and
        // `ServerGuard` can only kill the first. The servers survive holding their ports, so the
        // *next* test in the file hangs waiting for a health check that a zombie will never
        // answer. Addressing the CLI directly makes the child the server, so dropping the guard
        // actually stops it.
        let root = repo_root();
        assert!(
            root.join("node_modules/tsx/dist/cli.mjs").exists(),
            "node_modules/tsx is missing — run `npm ci` before the LAN tests"
        );
        // Both paths are given *relative* to the working directory set below, not absolute.
        // `repo_root()` canonicalizes, which on Windows yields an extended-length path
        // (`\?\C:\...`), and node cannot take one of those as its main module: it mis-parses it
        // and exits with `EISDIR: illegal operation on a directory, lstat 'C:'`.
        let child = Command::new("node")
            .arg("node_modules/tsx/dist/cli.mjs")
            .arg("server/index.ts")
            .current_dir(repo_root())
            .env("API_PORT", port.to_string())
            // Keep artifact writes inside the test's tempdir.
            .env("TACTIC_LAB_OUTPUT_DIR", output_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| panic!("failed to spawn node — is it on PATH? {e}"));
        ServerGuard(child)
    }

    fn wait_for_health(port: u16) {
        let client = reqwest::blocking::Client::new();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Ok(response) = client
                .get(format!("http://127.0.0.1:{port}/api/health"))
                .send()
            {
                if response.status().is_success() {
                    return;
                }
            }
            if Instant::now() > deadline {
                panic!("server never became healthy on port {port}");
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn fixture_output(session_id: &str, team_id: &str) -> Value {
        json!({
            "schemaVersion": "2.0", "taxonomyVersion": "tactics-v2",
            "interpretationMode": "deterministic-preview",
            "session": { "id": session_id, "title": "LAN sim", "durationMs": 0 },
            "teams": [
                { "id": "red", "playerIds": [1,2,3,4,5] },
                { "id": "yellow", "playerIds": [6,7,8,9,10] },
            ],
            "classification": {
                "primaryTactic": "highpress", "secondaryTraits": [], "alternativeTactics": [],
                "selectionReason": "system_fallback", "evidenceStrength": "weak",
                "explanation": "lan simulation fixture",
            },
            "summary": { "name": "LAN sim", "objective": "test" },
            "steps": [],
            "finalState": { "players": [], "ball": { "x": 0.5, "y": 0.5 }, "annotations": [] },
            "rlSelection": {
                "schemaVersion": "2.0", "taxonomyVersion": "tactics-v2", "sessionId": session_id,
                "primaryTactic": "highpress", "downstreamValue": "highpress", "teamId": team_id,
                "playerOverrides": [], "selectionReason": "system_fallback", "evidenceStrength": "weak",
            }
        })
    }

    fn drain_until<F>(runtime: &LobbyRuntime, timeout: Duration, mut found: F) -> Vec<LobbyEvent>
    where
        F: FnMut(&LobbyEvent) -> bool,
    {
        let deadline = Instant::now() + timeout;
        let mut all = Vec::new();
        loop {
            for event in runtime.poll() {
                let matched = found(&event);
                all.push(event);
                if matched {
                    return all;
                }
            }
            if Instant::now() > deadline {
                panic!(
                    "timed out waiting for expected lobby event; saw {} events",
                    all.len()
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// A real 1x1 PNG, so the server's magic-byte check is meaningful and a
    /// byte-for-byte comparison catches base64 corruption.
    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    /// Lays out one player's on-disk artifacts the way the real app does.
    fn stage_player_files(root: &std::path::Path, session_id: &str) -> (PathBuf, PathBuf) {
        let creation_dir = root.join("player-creations").join(session_id);
        std::fs::create_dir_all(&creation_dir).unwrap();
        std::fs::write(creation_dir.join("appearance.png"), TINY_PNG).unwrap();
        std::fs::write(creation_dir.join("superpower.png"), TINY_PNG).unwrap();
        std::fs::write(
            creation_dir.join("manifest.json"),
            json!({ "schemaVersion": 1, "sessionId": session_id }).to_string(),
        )
        .unwrap();

        let session_dir = root.join("sessions").join(session_id);
        std::fs::create_dir_all(&session_dir).unwrap();
        let session_path = session_dir.join("session.json");
        std::fs::write(
            &session_path,
            json!({ "schemaVersion": 1, "id": session_id, "events": [] }).to_string(),
        )
        .unwrap();
        (creation_dir, session_path)
    }

    fn entry_for(
        role: NetworkRole,
        match_id: &str,
        coaching_session: &str,
        creation_session: &str,
        staging: &std::path::Path,
        team: &str,
    ) -> MatchEntry {
        let (creation_dir, session_path) = stage_player_files(staging, creation_session);
        MatchEntry {
            role,
            match_id: match_id.to_owned(),
            tactical_output: fixture_output(coaching_session, team),
            coaching_session_id: coaching_session.to_owned(),
            creation_session_id: Some(creation_session.to_owned()),
            creation_dir: Some(creation_dir),
            session_json_path: Some(session_path),
        }
    }

    fn connect_both(addr: &str) -> (LobbyRuntime, LobbyRuntime, String) {
        let mut host_runtime = LobbyRuntime::default();
        let mut joiner_runtime = LobbyRuntime::default();

        host_runtime.announce_host(addr.to_owned());
        let host_events = drain_until(&host_runtime, Duration::from_secs(10), |event| {
            matches!(event, LobbyEvent::Connected { .. })
        });
        joiner_runtime.join(addr.to_owned());
        drain_until(&joiner_runtime, Duration::from_secs(10), |event| {
            matches!(event, LobbyEvent::Connected { .. })
        });

        let match_id = host_events
            .into_iter()
            .find_map(|event| match event {
                LobbyEvent::Connected { match_id, .. } => match_id,
                _ => None,
            })
            .expect("the server must hand back a match id on join");

        host_runtime.ensure_listening(addr.to_owned());
        joiner_runtime.ensure_listening(addr.to_owned());
        drain_until(&host_runtime, Duration::from_secs(10), |event| {
            matches!(
                event,
                LobbyEvent::Status(status)
                    if status.host_connected && status.joiner_connected
            )
        });
        (host_runtime, joiner_runtime, match_id)
    }

    /// The regression that mattered: the host and joiner each coach their own
    /// session, so the two halves of a match carry DIFFERENT session ids. The
    /// previous version of this test used one shared id and could never have
    /// caught the bug that made every real LAN match fail to start.
    #[test]
    fn host_and_joiner_reach_match_start_with_separate_sessions() {
        let port = 21_000 + (std::process::id() % 4000) as u16;
        let output_dir = tempfile::tempdir().unwrap();
        let staging = tempfile::tempdir().unwrap();
        let _server = spawn_server(port, output_dir.path());
        wait_for_health(port);

        let addr = format!("127.0.0.1:{port}");
        let (host_runtime, joiner_runtime, match_id) = connect_both(&addr);

        let host_session = "lan-sim-host-session";
        let joiner_session = "lan-sim-joiner-session";

        host_runtime.submit_match_entry(
            addr.clone(),
            entry_for(
                NetworkRole::Host,
                &match_id,
                host_session,
                "creation-host",
                staging.path(),
                "red",
            ),
        );
        joiner_runtime.submit_match_entry(
            addr.clone(),
            entry_for(
                NetworkRole::Joiner,
                &match_id,
                joiner_session,
                "creation-joiner",
                staging.path(),
                "yellow",
            ),
        );

        for runtime in [&host_runtime, &joiner_runtime] {
            let events = drain_until(runtime, Duration::from_secs(15), |event| {
                matches!(event, LobbyEvent::MatchStart { .. })
            });
            let Some(LobbyEvent::MatchStart {
                match_id: started,
                red,
                yellow,
            }) = events
                .into_iter()
                .find(|event| matches!(event, LobbyEvent::MatchStart { .. }))
            else {
                panic!("expected a MatchStart event");
            };

            assert_eq!(started, match_id);
            assert_eq!(red["session"]["id"], host_session);
            assert_eq!(yellow["session"]["id"], joiner_session);
            assert_ne!(red["session"]["id"], yellow["session"]["id"]);

            let handoff = build_match_handoff(&started, &red, &yellow)
                .expect("two independently-sessioned outputs must merge");
            assert_eq!(handoff.red.session_id, host_session);
            assert_eq!(handoff.yellow.session_id, joiner_session);
            let highpress = cube_soccer::systems::Tactic::HighPress.params();
            assert_eq!(handoff.team_tactics().orange.base_params(), highpress);
            assert_eq!(handoff.team_tactics().blue.base_params(), highpress);
        }
    }

    /// Both players' data must be on the host's disk before the match starts —
    /// that is the whole point of blocking on the upload.
    #[test]
    fn artifact_bundles_land_on_the_host_before_the_match_starts() {
        let port = 25_000 + (std::process::id() % 4000) as u16;
        let output_dir = tempfile::tempdir().unwrap();
        let staging = tempfile::tempdir().unwrap();
        let _server = spawn_server(port, output_dir.path());
        wait_for_health(port);

        let addr = format!("127.0.0.1:{port}");
        let (host_runtime, joiner_runtime, match_id) = connect_both(&addr);

        host_runtime.submit_match_entry(
            addr.clone(),
            entry_for(
                NetworkRole::Host,
                &match_id,
                "host-session",
                "creation-host",
                staging.path(),
                "red",
            ),
        );
        joiner_runtime.submit_match_entry(
            addr.clone(),
            entry_for(
                NetworkRole::Joiner,
                &match_id,
                "joiner-session",
                "creation-joiner",
                staging.path(),
                "yellow",
            ),
        );

        drain_until(&host_runtime, Duration::from_secs(15), |event| {
            matches!(event, LobbyEvent::MatchStart { .. })
        });

        let match_dir = output_dir.path().join("matches").join(&match_id);
        assert!(
            match_dir.join("match.json").is_file(),
            "match index missing at {}",
            match_dir.display()
        );
        for role in ["host", "joiner"] {
            let dir = match_dir.join(role);
            for file in [
                "session.json",
                "tactical-output.json",
                "manifest.json",
                "appearance.png",
                "superpower.png",
            ] {
                assert!(
                    dir.join(file).is_file(),
                    "{role}/{file} was not written to the host"
                );
            }
            // Byte-for-byte: catches base64 corruption in the upload path.
            assert_eq!(std::fs::read(dir.join("appearance.png")).unwrap(), TINY_PNG);
        }

        let index: Value =
            serde_json::from_slice(&std::fs::read(match_dir.join("match.json")).unwrap()).unwrap();
        assert_eq!(index["players"]["host"]["coachingSessionId"], "host-session");
        assert_eq!(
            index["players"]["joiner"]["coachingSessionId"],
            "joiner-session"
        );
    }

    /// Posting `ready` without artifacts must never start a match.
    #[test]
    fn ready_without_artifacts_never_starts_the_match() {
        let port = 29_000 + (std::process::id() % 3000) as u16;
        let output_dir = tempfile::tempdir().unwrap();
        let _server = spawn_server(port, output_dir.path());
        wait_for_health(port);

        let addr = format!("127.0.0.1:{port}");
        let (host_runtime, _joiner_runtime, _match_id) = connect_both(&addr);

        for (role, team, session) in [("host", "red", "a"), ("joiner", "yellow", "b")] {
            let body = json!({
                "role": role,
                "teamId": team,
                "tacticalOutput": fixture_output(session, team),
            });
            let outcome = post_json(&addr, "/api/lobby/ready", &body);
            assert!(
                outcome.is_err(),
                "ready without an artifact bundle must be rejected"
            );
        }

        std::thread::sleep(Duration::from_secs(2));
        assert!(
            !host_runtime
                .poll()
                .iter()
                .any(|event| matches!(event, LobbyEvent::MatchStart { .. })),
            "the match must not start until both artifact bundles are stored"
        );
    }
}
