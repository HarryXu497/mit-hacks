//! Private, cosmetic live feedback. This module only reads players and never touches tactics.
use crate::{network::NetworkRole, phase::AppPhase, theme};
use base64::Engine;
use bevy::{prelude::*, window::PrimaryWindow};
use bevy_egui::{egui, EguiContexts};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    FromSample, Sample, SizedSample,
};
use crossbeam_channel::{unbounded, Receiver, Sender};
use cube_soccer::{
    entities::{character::PlayerVisual, CubePlayer},
    game::config::CUBE_SIZE,
    tactics::speech::{Reply, SpeechRuntime, SpeechStatus},
};
use serde::Deserialize;
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

/// The shout's own microphone.
///
/// `tactics::speech::register` drains the table's `SpeechRuntime` every frame in *every* phase --
/// deliberately, so the table's closing words still land after the coach walks away. Sharing one
/// runtime would therefore let `receive_speech` swallow a shout's transcript during gameplay, so
/// the shout panel owns a second one and drains it itself.
#[derive(Resource, Default, Deref, DerefMut)]
struct ShoutMic(SpeechRuntime);

pub struct ShoutPlugin;
impl Plugin for ShoutPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShoutRuntime>()
            .init_resource::<ShoutMic>()
            .add_systems(OnEnter(AppPhase::Game), reset)
            .add_systems(OnExit(AppPhase::Game), reset)
            .add_systems(
                Update,
                (receive, panel).chain().run_if(in_state(AppPhase::Game)),
            )
            .add_systems(
                PostUpdate,
                badges
                    .after(bevy::transform::TransformSystem::TransformPropagate)
                    .before(bevy_egui::EguiSet::ProcessOutput)
                    .run_if(in_state(AppPhase::Game)),
            );
    }
}

#[derive(Debug, PartialEq, Eq, Default)]
enum Stage {
    #[default]
    Idle,
    Capturing,
    Finalizing,
    Generating,
    Speaking,
}
#[derive(Default)]
struct Transcript {
    finals: Vec<String>,
    seen: HashSet<String>,
    partial: String,
}
impl Transcript {
    fn push(&mut self, id: String, text: String) {
        self.partial.clear();
        if !text.trim().is_empty() && self.seen.insert(id) {
            self.finals.push(text);
        }
    }
    fn text(&self) -> String {
        self.finals.join(" ")
    }
    fn preview(&self) -> String {
        [self.text(), self.partial.clone()]
            .into_iter()
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}
#[derive(Resource)]
struct ShoutRuntime {
    stage: Stage,
    transcript: Transcript,
    reply: String,
    notice: String,
    selected: Option<usize>,
    started: Option<Instant>,
    epoch: Arc<AtomicU64>,
    tx: Sender<WorkerReply>,
    rx: Receiver<WorkerReply>,
}
impl Default for ShoutRuntime {
    fn default() -> Self {
        let (tx, rx) = unbounded();
        Self {
            stage: Stage::Idle,
            transcript: Transcript::default(),
            reply: String::new(),
            notice: String::new(),
            selected: None,
            started: None,
            epoch: Arc::new(AtomicU64::new(0)),
            tx,
            rx,
        }
    }
}
impl ShoutRuntime {
    fn cancel(&mut self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        self.stage = Stage::Idle;
        self.started = None;
        self.selected = None;
    }
    fn capture_action(&self, focused: bool, held: bool, now: Instant) -> CaptureAction {
        if self.stage != Stage::Capturing {
            return CaptureAction::None;
        }
        if !focused {
            return CaptureAction::Cancel;
        }
        if !held
            || self
                .started
                .is_some_and(|start| now.duration_since(start) >= Duration::from_secs(15))
        {
            return CaptureAction::Submit;
        }
        CaptureAction::None
    }
    fn begin(&mut self) -> bool {
        if self.stage != Stage::Idle {
            return false;
        }
        self.cancel();
        self.transcript = Transcript::default();
        self.reply.clear();
        self.notice.clear();
        self.stage = Stage::Capturing;
        self.started = Some(Instant::now());
        true
    }
}
#[derive(Debug, PartialEq, Eq)]
enum CaptureAction {
    None,
    Cancel,
    Submit,
}
impl Drop for ShoutRuntime {
    fn drop(&mut self) {
        self.cancel();
    }
}
struct WorkerReply {
    generation: u64,
    event: WorkerEvent,
}
enum WorkerEvent {
    Text(ShoutResponse),
    Finished(Option<String>),
    Failed(String),
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShoutResponse {
    request_id: String,
    player_number: usize,
    reply_text: String,
    audio: Option<Audio>,
    audio_error: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Audio {
    pcm16: String,
    encoding: String,
    sample_rate: u32,
    channels: u16,
}

fn reset(mut shout: ResMut<ShoutRuntime>, mut mic: ResMut<ShoutMic>) {
    shout.cancel();
    mic.cancel();
    shout.transcript = Transcript::default();
    shout.reply.clear();
    shout.notice.clear();
}

fn receive(
    mut shout: ResMut<ShoutRuntime>,
    mut mic: ResMut<ShoutMic>,
    role: Res<NetworkRole>,
) {
    if matches!(shout.stage, Stage::Capturing | Stage::Finalizing) {
        for event in mic.drain() {
            match event {
                Reply::Partial { text, .. } => shout.transcript.partial = text,
                Reply::Final { item_id, text, .. } => shout.transcript.push(item_id, text),
                _ => {}
            }
        }
        if mic.status == SpeechStatus::Error {
            shout.notice = mic
                .error
                .clone()
                .unwrap_or_else(|| "Couldn't hear you. Try again.".into());
            shout.stage = Stage::Idle;
            shout.started = None;
        } else if shout.stage == Stage::Finalizing && mic.status == SpeechStatus::Idle {
            let feedback = shout.transcript.text();
            info!("shout heard: {feedback:?}");
            shout.selected = target(&feedback);
            shout.stage = Stage::Generating;
            let generation = shout.epoch.load(Ordering::SeqCst);
            let team = if role.is_joiner() { "yellow" } else { "red" };
            let epoch = shout.epoch.clone();
            let tx = shout.tx.clone();
            let endpoint = std::env::var("TACTIC_LAB_API_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8787".into());
            std::thread::spawn(move || {
                request_reply(endpoint, team, feedback, generation, epoch, tx)
            });
        }
    }
    while let Ok(reply) = shout.rx.try_recv() {
        if reply.generation != shout.epoch.load(Ordering::SeqCst) {
            continue;
        }
        match reply.event {
            WorkerEvent::Text(response) => {
                // The server resolved again; follow it rather than the local guess.
                shout.selected = Some(response.player_number);
                shout.reply = format!("Player {}: {}", response.player_number, response.reply_text);
                shout.notice = response.audio_error.unwrap_or_default();
                shout.stage = Stage::Speaking;
            }
            WorkerEvent::Finished(error) => {
                if let Some(error) = error {
                    shout.notice = error;
                }
                shout.stage = Stage::Idle;
                shout.selected = None;
            }
            WorkerEvent::Failed(error) => {
                shout.notice = error;
                shout.stage = Stage::Idle;
                shout.selected = None;
            }
        }
    }
}

fn panel(
    mut contexts: EguiContexts,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut shout: ResMut<ShoutRuntime>,
    mut mic: ResMut<ShoutMic>,
) {
    let Ok(window) = windows.get_single() else {
        return;
    };
    let ctx = contexts.ctx_mut();
    match shout.capture_action(
        window.focused,
        ctx.input(|i| i.pointer.primary_down()),
        Instant::now(),
    ) {
        CaptureAction::Cancel => {
            shout.cancel();
            mic.cancel();
            shout.notice = "Shout canceled when the window lost focus.".into();
        }
        CaptureAction::Submit => {
            mic.hush();
            shout.stage = Stage::Finalizing;
            shout.started = None;
        }
        CaptureAction::None => {}
    }
    egui::Area::new(egui::Id::new("shout-panel"))
        .anchor(egui::Align2::RIGHT_BOTTOM, [-14.0, -14.0])
        .show(ctx, |ui| {
            // The theme's timber and canopy, on the theme's frame, but pulled in tight: this sits
            // over live play and should read as a corner label, not a second screen.
            theme::panel_frame()
                .inner_margin(egui::Margin::symmetric(11.0, 8.0))
                .show(ui, |ui| {
                    ui.set_width(196.0);
                    ui.label(
                        egui::RichText::new("SHOUT")
                            .size(10.0)
                            .color(theme::CLOTH_DIM),
                    );
                    let label = match shout.stage {
                        Stage::Idle => "Hold to shout",
                        Stage::Capturing if mic.status == SpeechStatus::Connecting => {
                            "Connecting…"
                        }
                        Stage::Capturing => "Listening…",
                        Stage::Finalizing => "Finishing…",
                        Stage::Generating => "Replying…",
                        Stage::Speaking => "Speaking…",
                    };
                    let button = ui
                        .add_enabled_ui(
                            shout.stage == Stage::Idle || shout.stage == Stage::Capturing,
                            |ui| {
                                theme::slab_compact(
                                    ui,
                                    label,
                                    theme::Tone::Primary,
                                    shout.stage == Stage::Capturing,
                                )
                            },
                        )
                        .inner;
                    if window.focused
                        && button.contains_pointer()
                        && ctx.input(|i| i.pointer.primary_pressed())
                        && shout.begin()
                    {
                        let id = format!("shout-{}", uuid::Uuid::new_v4());
                        mic.listen(&id, 0);
                    }
                    // The how-to earns its space once. After the first shout the panel is just
                    // the button and whatever was said back.
                    if shout.stage == Stage::Idle && shout.reply.is_empty() {
                        ui.label(
                            egui::RichText::new("“Player 4, get back!”")
                                .size(10.0)
                                .color(theme::CLOTH_DIM),
                        );
                    }
                    let preview = shout.transcript.preview();
                    if !preview.is_empty() {
                        ui.label(
                            egui::RichText::new(format!("You: {preview}"))
                                .size(11.0)
                                .color(theme::CLOTH_DIM),
                        );
                    }
                    if !shout.reply.is_empty() {
                        ui.label(
                            egui::RichText::new(&shout.reply)
                                .size(11.5)
                                .color(theme::GOLD),
                        );
                    }
                    if !shout.notice.is_empty() {
                        ui.label(
                            egui::RichText::new(&shout.notice)
                                .size(10.0)
                                .color(theme::CLOTH_DIM),
                        );
                    }
                });
        });
}

fn badges(
    mut contexts: EguiContexts,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    players: Query<(Entity, &CubePlayer, &GlobalTransform)>,
    visuals: Query<(&Parent, &GlobalTransform), With<PlayerVisual>>,
    role: Res<NetworkRole>,
    shout: Res<ShoutRuntime>,
    settings: Res<bevy_egui::EguiSettings>,
) {
    let Some((camera, camera_transform)) = cameras.iter().find(|(c, _)| c.is_active) else {
        return;
    };
    let Some(viewport) = camera.logical_viewport_rect() else {
        return;
    };
    let team = role.coached_side().game_team();
    let ctx = contexts.ctx_mut();
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("player-numbers"),
    ));
    let scale = settings.scale_factor as f32;
    let mut placed = Vec::new();
    for (entity, player, transform) in &players {
        if player.team != team {
            continue;
        }
        // Both base and forged GLBs are normalized to CUBE_SIZE, mounted on PlayerVisual.
        let head = visuals
            .iter()
            .find(|(parent, _)| parent.get() == entity)
            .map(|(_, t)| t.transform_point(Vec3::Y * (CUBE_SIZE * 0.65)))
            .unwrap_or_else(|| transform.translation() + Vec3::Y * (CUBE_SIZE * 0.65));
        let Some(ndc) = camera.world_to_ndc(camera_transform, head) else {
            continue;
        };
        if ndc.z < 0.0 || ndc.z > 1.0 || ndc.x.abs() > 1.0 || ndc.y.abs() > 1.0 {
            continue;
        }
        let Some(point) = camera.world_to_viewport(camera_transform, head) else {
            continue;
        };
        let number = player.index + 1;
        let active = shout.selected == Some(number);
        let style = badge_style(active);
        let anchor = egui::pos2(
            (point.x + viewport.min.x) / scale,
            (point.y + viewport.min.y) / scale - 12.0,
        );
        let point = place_badge(anchor, style.radius, &placed, ctx.screen_rect());
        if point.distance(anchor) > 1.0 {
            painter.line_segment([anchor, point], egui::Stroke::new(1.0_f32, style.leader));
        }
        placed.push((point, style.radius));
        painter.circle_filled(point, style.radius, style.fill);
        painter.circle_stroke(point, style.ring_radius, style.ring);
        painter.text(
            point,
            egui::Align2::CENTER_CENTER,
            number,
            egui::FontId::proportional(style.font),
            style.text,
        );
        if active {
            if let Some(body) = camera.world_to_viewport(camera_transform, transform.translation())
            {
                painter.circle_stroke(
                    egui::pos2(
                        (body.x + viewport.min.x) / scale,
                        (body.y + viewport.min.y) / scale,
                    ),
                    24.0,
                    egui::Stroke::new(2.0_f32, faded(theme::GOLD, 200)),
                );
            }
        }
    }
}

/// A palette colour at reduced strength, so a number sits in the scene instead of on top of it.
fn faded(color: egui::Color32, alpha: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

/// How a number reads on the pitch.
struct BadgeStyle {
    radius: f32,
    ring_radius: f32,
    fill: egui::Color32,
    ring: egui::Stroke,
    leader: egui::Color32,
    text: egui::Color32,
    font: f32,
}

/// At rest a number is the same ink the world already outlines every character with, small and
/// half-lit, so five of them read as markings rather than as five lit-up controls. Gold is the
/// palette's "chosen" accent, so only the player being addressed takes it — which is what makes
/// the highlight legible at a glance.
fn badge_style(active: bool) -> BadgeStyle {
    if active {
        BadgeStyle {
            radius: 15.0,
            ring_radius: 17.0,
            fill: theme::GOLD,
            ring: egui::Stroke::new(1.5_f32, theme::GOLD),
            leader: theme::GOLD,
            text: theme::INK,
            font: 16.0,
        }
    } else {
        BadgeStyle {
            radius: 9.0,
            ring_radius: 9.0,
            fill: faded(theme::INK, 205),
            ring: egui::Stroke::new(1.0_f32, faded(theme::CLOTH_DIM, 60)),
            leader: faded(theme::CLOTH_DIM, 55),
            text: faded(theme::CLOTH_DIM, 225),
            font: 11.0,
        }
    }
}

// Keep all five numbers readable when athletes bunch together; the leader points back to the head.
fn place_badge(
    anchor: egui::Pos2,
    radius: f32,
    placed: &[(egui::Pos2, f32)],
    bounds: egui::Rect,
) -> egui::Pos2 {
    let vertical = if anchor.y < bounds.top() + radius + 120.0 {
        1.0
    } else {
        -1.0
    };
    for row in 0..5 {
        for column in [0.0, -1.0, 1.0, -2.0, 2.0] {
            let point = bounds
                .shrink(radius)
                .clamp(anchor + egui::vec2(column * 30.0, vertical * (row as f32) * 30.0));
            if placed
                .iter()
                .all(|(other, other_radius)| point.distance(*other) >= radius + other_radius + 3.0)
            {
                return point;
            }
        }
    }
    anchor
}

// A first guess for the highlight, so a badge lights the moment you release when the coach named
// someone. Nobody named means no guess: the server picks whoever looks up, and its answer wins.
fn target(text: &str) -> Option<usize> {
    let tokens: Vec<String> = text
        .to_lowercase()
        .split_whitespace()
        .map(|s| {
            s.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '.')
                .trim_end_matches('.')
                .to_owned()
        })
        .collect();
    for (i, token) in tokens.iter().enumerate() {
        // How a coach actually addresses someone mid-match: "player 4", "number 4", "monkey 4".
        // An address word is still required, so a stray number ("back in two seconds") is not a
        // target. Kept in step with `resolveShout` in server/shout.ts, which resolves again.
        if !matches!(
            token.as_str(),
            "player" | "players" | "number" | "numbers" | "monkey" | "monkeys"
        ) {
            continue;
        }
        let mut j = i + 1;
        if tokens.get(j).is_some_and(|t| t == "number" || t.is_empty()) {
            j += 1;
        }
        if let Some(n) = tokens.get(j).and_then(|t| player_number(t)) {
            if (1..=5).contains(&n) {
                return Some(n as usize);
            }
        }
    }
    None
}
fn player_number(word: &str) -> Option<i32> {
    [
        "zero",
        "one",
        "two",
        "three",
        "four",
        "five",
        "six",
        "seven",
        "eight",
        "nine",
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
        "twenty",
    ]
    .iter()
    .position(|w| *w == word)
    .map(|n| n as i32)
    .or_else(|| word.parse().ok())
}

fn request_reply(
    endpoint: String,
    team: &str,
    feedback: String,
    generation: u64,
    epoch: Arc<AtomicU64>,
    tx: Sender<WorkerReply>,
) {
    let send = |event| {
        let _ = tx.send(WorkerReply { generation, event });
    };
    let outcome = (|| -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let id = uuid::Uuid::new_v4().to_string();
        let response = runtime.block_on(async {
            let client = reqwest::Client::builder().timeout(Duration::from_secs(38)).build().map_err(|e| e.to_string())?;
            let request = async {
                let response = client.post(format!("{}/api/shout", endpoint.trim_end_matches('/')))
                    .json(&serde_json::json!({ "requestId": id, "teamId": team, "transcript": feedback }))
                    .send().await.map_err(|_| "Couldn't reach the shout service. Try again.".to_owned())?;
                if !response.status().is_success() {
                    let error: serde_json::Value = response.json().await.unwrap_or_default();
                    return Err(error["message"].as_str().unwrap_or("The player couldn't reply. Try again.").to_owned());
                }
                response.json::<ShoutResponse>().await.map_err(|_| "The shout service returned an invalid reply.".to_owned())
            };
            tokio::pin!(request);
            loop {
                tokio::select! {
                    result = &mut request => break result,
                    _ = tokio::time::sleep(Duration::from_millis(20)) => {
                        if epoch.load(Ordering::SeqCst) != generation { return Err("Canceled".into()); }
                    }
                }
            }
        })?;
        if epoch.load(Ordering::SeqCst) != generation {
            return Ok(());
        }
        if response.request_id != id || response.reply_text.trim().is_empty() {
            return Err("The shout service returned a mismatched reply.".into());
        }
        let mut response = response;
        let audio = response.audio.take();
        info!(
            "shout reply: player {} said {:?}{}",
            response.player_number,
            response.reply_text,
            match (&audio, &response.audio_error) {
                (Some(_), _) => String::new(),
                (None, Some(error)) => format!(" (no audio: {error})"),
                (None, None) => " (no audio)".to_owned(),
            }
        );
        send(WorkerEvent::Text(response));
        let audio_error = audio.and_then(|audio| play(audio, generation, epoch.clone()).err());
        send(WorkerEvent::Finished(audio_error));
        Ok(())
    })();
    if let Err(error) = outcome {
        send(WorkerEvent::Failed(error));
    }
}

fn play(audio: Audio, generation: u64, epoch: Arc<AtomicU64>) -> Result<(), String> {
    let result = (|| -> anyhow::Result<()> {
        anyhow::ensure!(
            audio.encoding == "linear16" && audio.sample_rate == 24000 && audio.channels == 1,
            "Unexpected audio format"
        );
        let bytes = base64::engine::general_purpose::STANDARD.decode(audio.pcm16)?;
        anyhow::ensure!(
            !bytes.is_empty() && bytes.len() % 2 == 0 && bytes.len() <= 1_440_000,
            "Invalid audio"
        );
        let samples: Vec<f32> = bytes
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
            .collect();
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| anyhow::anyhow!("No output device"))?;
        let supported = device.default_output_config()?;
        info!(
            "shout voice: {} samples ({:.1}s) -> {} @ {}Hz {:?}",
            samples.len(),
            samples.len() as f32 / audio.sample_rate as f32,
            device.name().unwrap_or_else(|_| "<unnamed>".into()),
            supported.sample_rate().0,
            supported.sample_format(),
        );
        let config: cpal::StreamConfig = supported.clone().into();
        let (done_tx, done_rx) = unbounded();
        let duration = samples.len() as f64 / audio.sample_rate as f64;
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => output::<f32>(
                &device,
                &config,
                samples,
                audio.sample_rate,
                generation,
                epoch.clone(),
                done_tx,
            )?,
            cpal::SampleFormat::I16 => output::<i16>(
                &device,
                &config,
                samples,
                audio.sample_rate,
                generation,
                epoch.clone(),
                done_tx,
            )?,
            cpal::SampleFormat::U16 => output::<u16>(
                &device,
                &config,
                samples,
                audio.sample_rate,
                generation,
                epoch.clone(),
                done_tx,
            )?,
            _ => anyhow::bail!("Unsupported output format"),
        };
        if epoch.load(Ordering::SeqCst) != generation {
            return Ok(());
        }
        stream.play()?;
        let start = Instant::now();
        while epoch.load(Ordering::SeqCst) == generation {
            match done_rx.recv_timeout(Duration::from_millis(20)) {
                Ok(Ok(())) => break,
                Ok(Err(error)) => anyhow::bail!(error),
                Err(_) if start.elapsed().as_secs_f64() > duration + 3.0 => {
                    anyhow::bail!("Playback timed out")
                }
                Err(_) => {}
            }
        }
        drop(stream);
        Ok(())
    })();
    // The coach only needs "Voice unavailable"; whoever is debugging needs the reason.
    result.map_err(|error| {
        warn!("shout playback failed: {error:#}");
        "Voice unavailable".into()
    })
}

fn output<T: SizedSample + FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    samples: Vec<f32>,
    rate: u32,
    generation: u64,
    epoch: Arc<AtomicU64>,
    done: Sender<Result<(), String>>,
) -> Result<cpal::Stream, cpal::BuildStreamError> {
    let channels = config.channels as usize;
    let ratio = rate as f64 / config.sample_rate.0 as f64;
    let mut position = 0.0_f64;
    // Emit a small silent tail so the last queued audio reaches the device before dropping it.
    let end = samples.len() as f64 + rate as f64 * 0.15;
    let mut reported = false;
    let errors = done.clone();
    device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            let canceled = epoch.load(Ordering::SeqCst) != generation;
            for frame in data.chunks_mut(channels) {
                let lower = position as usize;
                let a = samples.get(lower).copied().unwrap_or(0.0);
                let b = samples.get(lower + 1).copied().unwrap_or(a);
                let value = if canceled {
                    0.0
                } else {
                    a + (b - a) * (position.fract() as f32)
                };
                for sample in frame {
                    *sample = value.to_sample::<T>();
                }
                position += ratio;
            }
            if !reported && (canceled || position >= end) {
                let _ = done.send(Ok(()));
                reported = true;
            }
        },
        move |error| {
            let _ = errors.send(Err(error.to_string()));
        },
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn targets_are_local_and_explicit() {
        for text in [
            "Player 4, get back!",
            "player four get back",
            "Player four, player 4, move!",
            "player number 4 move",
            "player #4 move",
            "Number four, get back!",
            "number 4 get back",
            "Monkey four, get back!",
            "monkey number 4 move",
            // The first player named wins rather than the shout being refused.
            "player 4 and player 3 move",
            "players 4 and 3 move",
        ] {
            assert_eq!(target(text), Some(4), "{text}");
        }
        // Nobody on the roster named: no local guess, so the badge waits for the server's pick
        // instead of lighting the wrong monkey.
        for text in ["", "get back", "player six move", "player 0 move", "run at them"] {
            assert_eq!(target(text), None, "{text}");
        }
        assert_eq!(target("Player four, get back in two seconds"), Some(4));
        assert_eq!(
            NetworkRole::Joiner.coached_side().game_team(),
            cube_soccer::Team::Blue
        );
        assert_eq!(
            NetworkRole::Host.coached_side().game_team(),
            cube_soccer::Team::Orange
        );
    }
    #[test]
    fn aggregates_sentences_and_deduplicates_events() {
        let mut transcript = Transcript::default();
        transcript.push("a".into(), "Player four.".into());
        transcript.push("a".into(), "Player four.".into());
        transcript.partial = "Get back".into();
        assert_eq!(transcript.preview(), "Player four. Get back");
        transcript.push("b".into(), "Get back!".into());
        assert_eq!(transcript.text(), "Player four. Get back!");
    }
    #[test]
    fn capture_releases_anywhere_cancels_on_blur_and_limits_to_fifteen_seconds() {
        let mut shout = ShoutRuntime::default();
        shout.begin();
        let start = shout.started.unwrap();
        assert_eq!(
            shout.capture_action(true, true, start + Duration::from_secs(14)),
            CaptureAction::None
        );
        assert_eq!(
            shout.capture_action(true, true, start + Duration::from_secs(15)),
            CaptureAction::Submit
        );
        assert_eq!(
            shout.capture_action(true, false, start),
            CaptureAction::Submit
        );
        assert_eq!(
            shout.capture_action(false, false, start),
            CaptureAction::Cancel
        );
        shout.stage = Stage::Generating;
        assert_eq!(
            shout.capture_action(false, false, start),
            CaptureAction::None
        );
    }
    #[test]
    fn a_resting_number_is_quieter_than_the_one_being_addressed() {
        let resting = badge_style(false);
        let addressed = badge_style(true);
        assert!(resting.radius < addressed.radius);
        assert!(resting.font < addressed.font);
        // Gold means "chosen" in this palette, so a number at rest must not claim it.
        assert_ne!(resting.fill, theme::GOLD);
        assert_eq!(addressed.fill, theme::GOLD);
        // And it sits in the scene rather than on top of it.
        assert!(resting.fill.a() < 255);
        assert!(resting.ring.color.a() < resting.text.a());
        assert!(resting.leader.a() < addressed.leader.a());
    }

    #[test]
    fn clustered_badges_remain_distinct_inside_a_resized_window() {
        let bounds = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        for anchor in [egui::pos2(320.0, 240.0), egui::pos2(639.0, 1.0)] {
            let mut placed = Vec::new();
            for _ in 0..5 {
                let point = place_badge(anchor, 11.0, &placed, bounds);
                assert!(bounds.shrink(11.0).contains(point));
                assert!(placed
                    .iter()
                    .all(|(other, _)| point.distance(*other) >= 25.0));
                placed.push((point, 11.0));
            }
        }
    }

    /// The whole reply path with no microphone and no window: a canned transcript goes through
    /// the real validator, the real HTTP request, and the real output device. Reaching the end
    /// means a player answered and the clip drained through the speakers.
    ///
    /// Opt-in: it needs `npm run dev:api` running, spends provider credit, and makes noise.
    ///   cargo test --manifest-path native/coaching/Cargo.toml --lib -p tactic-lab-native \
    ///     -- --ignored --nocapture shout_round_trip_speaks
    #[test]
    #[ignore]
    fn shout_round_trip_speaks() {
        let feedback = "Player 4, get back on defense!".to_owned();
        assert_eq!(target(&feedback), Some(4));

        let (tx, rx) = unbounded();
        let endpoint = std::env::var("TACTIC_LAB_API_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8787".into());
        // Synchronous, playback included, so everything is in the channel when it returns.
        request_reply(
            endpoint,
            "red",
            feedback,
            0,
            Arc::new(AtomicU64::new(0)),
            tx,
        );

        let (mut reply, mut finished) = (None, None);
        while let Ok(message) = rx.try_recv() {
            match message.event {
                WorkerEvent::Text(response) => reply = Some(response),
                WorkerEvent::Finished(error) => finished = Some(error),
                WorkerEvent::Failed(error) => panic!("the shout failed: {error}"),
            }
        }

        let reply = reply.expect("the service returned a reply");
        assert_eq!(reply.player_number, 4, "answered as the wrong player");
        assert!(!reply.reply_text.trim().is_empty(), "empty reply text");
        println!("player {} said: {}", reply.player_number, reply.reply_text);
        assert!(
            reply.audio_error.is_none(),
            "the service could not voice the reply: {:?}",
            reply.audio_error
        );
        // `request_reply` plays inline before reporting Finished, so no error here means the
        // output device accepted and drained the whole clip.
        assert_eq!(finished, Some(None), "playback reported a failure");
    }

    #[test]
    fn stale_results_do_not_reappear_after_leaving_gameplay() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<ShoutRuntime>()
            .init_resource::<ShoutMic>()
            .init_resource::<NetworkRole>()
            .add_systems(Update, receive);
        let mut shout = app.world.resource_mut::<ShoutRuntime>();
        shout.begin();
        let generation = shout.epoch.load(Ordering::SeqCst);
        shout.cancel();
        shout
            .tx
            .send(WorkerReply {
                generation,
                event: WorkerEvent::Text(ShoutResponse {
                    request_id: "old-round".into(),
                    player_number: 4,
                    reply_text: "Stale quip".into(),
                    audio: None,
                    audio_error: None,
                }),
            })
            .unwrap();
        drop(shout);
        app.update();
        let shout = app.world.resource::<ShoutRuntime>();
        assert!(shout.reply.is_empty());
        assert_eq!(shout.stage, Stage::Idle);
    }

    #[test]
    fn busy_presses_and_phase_cancellation() {
        let mut shout = ShoutRuntime::default();
        assert!(shout.begin());
        let generation = shout.epoch.load(Ordering::SeqCst);
        for stage in [
            Stage::Capturing,
            Stage::Finalizing,
            Stage::Generating,
            Stage::Speaking,
        ] {
            shout.stage = stage;
            assert!(!shout.begin());
        }
        shout.cancel();
        assert_ne!(shout.epoch.load(Ordering::SeqCst), generation);
        assert!(shout.begin());
    }
}
