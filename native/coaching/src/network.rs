//! LAN host/join multiplayer: the pre-coaching lobby (main menu, mDNS
//! discovery / manual IP connect) and the lobby protocol that merges each
//! side's finished tactical output before the Game phase starts.
use crate::game_handoff::{CoachedTeam, MatchHandoff, TeamSide};
use crate::interpretation::TacticalResult;
use crate::phase::AppPhase;
use crate::{CoachingLifecycle, EnterGame, SetCoachingActive};
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

#[derive(Resource, Default)]
pub struct LobbyUiState {
    pub screen: LobbyScreen,
    pub manual_addr: String,
    pub status: Option<String>,
    pub error: Option<String>,
    pub discovered: Vec<DiscoveredHost>,
    pub connecting: bool,
    pub waiting_for_match: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LobbyScreen {
    #[default]
    MainMenu,
    Hosting,
    Joining,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredHost {
    pub label: String,
    pub addr: String,
}

#[derive(Debug)]
enum LobbyEvent {
    Connected { host_addr: String },
    ConnectFailed(String),
    Status {
        host_connected: bool,
        joiner_connected: bool,
    },
    MatchStart { red: Value, yellow: Value },
    Error(String),
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
                Ok(_) => {
                    let _ = sender.send(LobbyEvent::Connected { host_addr: addr });
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
                Ok(_) => {
                    let _ = sender.send(LobbyEvent::Connected { host_addr });
                }
                Err(message) => {
                    let _ = sender.send(LobbyEvent::ConnectFailed(message));
                }
            }
        });
    }

    /// Submit this machine's finished tactical output for its coached team.
    pub fn submit_ready(&self, host_addr: String, role: NetworkRole, output: Value) {
        let sender = self.sender.clone();
        std::thread::spawn(move || {
            let role_label = if role.is_joiner() { "joiner" } else { "host" };
            let team_label = match role.coached_side() {
                TeamSide::Red => "red",
                TeamSide::Yellow => "yellow",
            };
            let body = json!({ "role": role_label, "teamId": team_label, "tacticalOutput": output });
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

    pub fn poll(&self) -> Vec<LobbyEventPublic> {
        let mut out = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            out.push(match event {
                LobbyEvent::Connected { host_addr } => LobbyEventPublic::Connected { host_addr },
                LobbyEvent::ConnectFailed(message) => LobbyEventPublic::ConnectFailed(message),
                LobbyEvent::Status {
                    host_connected,
                    joiner_connected,
                } => LobbyEventPublic::Status {
                    host_connected,
                    joiner_connected,
                },
                LobbyEvent::MatchStart { red, yellow } => {
                    LobbyEventPublic::MatchStart { red, yellow }
                }
                LobbyEvent::Error(message) => LobbyEventPublic::Error(message),
            });
        }
        out
    }
}

/// Public mirror of `LobbyEvent`; kept separate so other modules don't need
/// to depend on the private worker-thread message type directly.
pub enum LobbyEventPublic {
    Connected { host_addr: String },
    ConnectFailed(String),
    Status {
        host_connected: bool,
        joiner_connected: bool,
    },
    MatchStart { red: Value, yellow: Value },
    Error(String),
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
                                let _ = sender.send(LobbyEvent::Status {
                                    host_connected: value["hostConnected"].as_bool().unwrap_or(false),
                                    joiner_connected: value["joinerConnected"].as_bool().unwrap_or(false),
                                });
                            }
                            Some("start") => {
                                let _ = sender.send(LobbyEvent::MatchStart {
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
            .add_event::<MatchReady>()
            .add_systems(
                Update,
                lobby_menu_ui.run_if(in_state(AppPhase::Lobby)),
            )
            .add_systems(
                Update,
                poll_mdns_discoveries.run_if(in_state(AppPhase::Lobby)),
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
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.add_space(48.0);
        ui.vertical_centered(|ui| {
            ui.heading("Tactic Lab");
            ui.label("Coach a 5-v-5 tactic, alone or against a friend on the same wifi.");
            ui.add_space(24.0);

            match ui_state.screen {
                LobbyScreen::MainMenu => {
                    ui.set_max_width(320.0);
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
                    ui.add_space(8.0);
                    if big_button(ui, "Play Solo") {
                        *role = NetworkRole::Solo;
                        next_phase.set(AppPhase::Creation);
                    }
                }
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
/// the lobby connects (typically during Coaching, once both sides finish).
fn receive_lobby_events(
    mut commands: Commands,
    mut runtime: ResMut<LobbyRuntime>,
    mut ui_state: ResMut<LobbyUiState>,
    role: Res<NetworkRole>,
    mut endpoint: ResMut<NetworkEndpoint>,
    mut lifecycle: ResMut<CoachingLifecycle>,
    mut next_phase: ResMut<NextState<AppPhase>>,
    phase: Res<State<AppPhase>>,
) {
    for event in runtime.poll() {
        match event {
            LobbyEventPublic::Connected { host_addr } => {
                ui_state.connecting = false;
                ui_state.error = None;
                endpoint.host_addr = host_addr.clone();
                runtime.ensure_listening(host_addr.clone());
                if *role == NetworkRole::Joiner {
                    redirect_env_to_host(&host_addr);
                    next_phase.set(AppPhase::Creation);
                }
            }
            LobbyEventPublic::ConnectFailed(message) => {
                ui_state.connecting = false;
                ui_state.error = Some(message);
            }
            LobbyEventPublic::Status {
                host_connected,
                joiner_connected,
            } => {
                if *role == NetworkRole::Host {
                    ui_state.status = Some(if joiner_connected {
                        "Player connected!".into()
                    } else {
                        "Waiting for a player to join…".into()
                    });
                    if host_connected && joiner_connected && *phase.get() == AppPhase::Lobby {
                        next_phase.set(AppPhase::Creation);
                    }
                }
            }
            LobbyEventPublic::MatchStart { red, yellow } => {
                ui_state.waiting_for_match = false;
                match build_match_handoff(&red, &yellow) {
                    Ok(handoff) => {
                        commands.insert_resource(handoff.team_tactics());
                        commands.insert_resource(handoff);
                        lifecycle.active = false;
                        next_phase.set(AppPhase::Game);
                    }
                    Err(error) => {
                        ui_state.error = Some(format!("Cannot start match: {error:#}"));
                    }
                }
            }
            LobbyEventPublic::Error(message) => {
                ui_state.error = Some(message);
            }
        }
    }
}

/// Consumes the "ready to enter the game" signal from the results screen
/// (the "Next"/"Continue anyway" button). Solo just enters the game
/// directly, exactly like before multiplayer existed. Networked play
/// submits this machine's finished tactical output to the lobby instead —
/// the actual game entry happens later, once `receive_lobby_events` sees
/// the server's merged `MatchStart` push.
fn handle_match_ready(
    mut events: EventReader<MatchReady>,
    role: Res<NetworkRole>,
    endpoint: Res<NetworkEndpoint>,
    runtime: Res<LobbyRuntime>,
    mut ui_state: ResMut<LobbyUiState>,
    result: Res<TacticalResult>,
    mut enter_game: EventWriter<EnterGame>,
    mut set_active: EventWriter<SetCoachingActive>,
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
    ui_state.waiting_for_match = true;
    ui_state.status = Some("Submitting your tactic — waiting for the other player…".into());
    set_active.send(SetCoachingActive(false));
    runtime.submit_ready(endpoint.host_addr.clone(), *role, output);
}

fn build_match_handoff(red: &Value, yellow: &Value) -> anyhow::Result<MatchHandoff> {
    let session_id = red["session"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing session id in red output"))?;
    let red_team = CoachedTeam::from_output_for_team(red, session_id, TeamSide::Red)?;
    let yellow_team = CoachedTeam::from_output_for_team(yellow, session_id, TeamSide::Yellow)?;
    Ok(MatchHandoff {
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

fn big_button(ui: &mut egui::Ui, label: &str) -> bool {
    ui.add_sized(
        egui::vec2(ui.available_width(), 48.0),
        egui::Button::new(egui::RichText::new(label).strong()),
    )
    .clicked()
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

    fn spawn_server(port: u16) -> ServerGuard {
        let child = Command::new("npx")
            .args(["tsx", "server/index.ts"])
            .current_dir(repo_root())
            .env("API_PORT", port.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn `npx tsx server/index.ts` — is npm/node on PATH?");
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

    fn drain_until<F>(runtime: &LobbyRuntime, timeout: Duration, mut found: F) -> Vec<LobbyEventPublic>
    where
        F: FnMut(&LobbyEventPublic) -> bool,
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
                panic!("timed out waiting for expected lobby event; saw {} events", all.len());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn host_and_joiner_reach_match_start_over_real_sockets() {
        let port = 21_000 + (std::process::id() % 4000) as u16;
        let _server = spawn_server(port);
        wait_for_health(port);

        let addr = format!("127.0.0.1:{port}");
        let session_id = "lan-sim-session";

        let mut host_runtime = LobbyRuntime::default();
        let mut joiner_runtime = LobbyRuntime::default();

        // Both machines "connect" to the lobby, exactly like the Lobby UI does.
        host_runtime.announce_host(addr.clone());
        drain_until(&host_runtime, Duration::from_secs(10), |event| {
            matches!(event, LobbyEventPublic::Connected { .. })
        });
        joiner_runtime.join(addr.clone());
        drain_until(&joiner_runtime, Duration::from_secs(10), |event| {
            matches!(event, LobbyEventPublic::Connected { .. })
        });

        // Each opens its persistent WS listener, as `receive_lobby_events`
        // would upon seeing `Connected` — this is how both sides learn the
        // other has joined, and later receive the merged `MatchStart` push.
        host_runtime.ensure_listening(addr.clone());
        joiner_runtime.ensure_listening(addr.clone());

        drain_until(&host_runtime, Duration::from_secs(10), |event| {
            matches!(
                event,
                LobbyEventPublic::Status { host_connected: true, joiner_connected: true }
            )
        });

        // Each side submits the tactical output it actually produced from
        // its own coaching session, exactly as `handle_match_ready` does.
        host_runtime.submit_ready(
            addr.clone(),
            NetworkRole::Host,
            fixture_output(session_id, "red"),
        );
        joiner_runtime.submit_ready(
            addr.clone(),
            NetworkRole::Joiner,
            fixture_output(session_id, "yellow"),
        );

        let host_events = drain_until(&host_runtime, Duration::from_secs(10), |event| {
            matches!(event, LobbyEventPublic::MatchStart { .. })
        });
        let joiner_events = drain_until(&joiner_runtime, Duration::from_secs(10), |event| {
            matches!(event, LobbyEventPublic::MatchStart { .. })
        });

        for events in [host_events, joiner_events] {
            let Some(LobbyEventPublic::MatchStart { red, yellow }) = events
                .into_iter()
                .find(|event| matches!(event, LobbyEventPublic::MatchStart { .. }))
            else {
                panic!("expected a MatchStart event");
            };
            assert_eq!(red["rlSelection"]["teamId"], "red");
            assert_eq!(yellow["rlSelection"]["teamId"], "yellow");
            assert_eq!(red["session"]["id"], session_id);
            assert_eq!(yellow["session"]["id"], session_id);

            let handoff = build_match_handoff(&red, &yellow)
                .expect("merged red+yellow output should build a valid MatchHandoff");
            let tactics = handoff.team_tactics();
            let highpress = cube_soccer::systems::Tactic::HighPress.params();
            assert_eq!(tactics.orange.base_params(), highpress);
            assert_eq!(tactics.blue.base_params(), highpress);
        }
    }
}
