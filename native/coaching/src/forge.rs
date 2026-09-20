//! The drawn superpower, made real.
//!
//! The player paints a superpower at the easel. This asks the local service which of the game's
//! four powers it is, and then arms their whole side with it for the match.
//!
//! Why the whole side and not one player: there is one drawing per session, and in a five-a-side
//! match watched from a broadcast camera a power held by a single cube is almost impossible to
//! notice. Arming the side makes the thing you drew visible in play, which is the point of having
//! drawn it. The same reasoning put the generated *character* on the whole side.
//!
//! The classification runs in the service rather than here, for the same reason transcription
//! does: that process owns the machine-local Python toolchain, and a joining machine already
//! redirects its API calls to the host, so a LAN guest needs no Python of its own.
//!
//! Nothing here is load-bearing for a match. If the service is down, or Python is missing, or the
//! answer is a power this build does not have, the match still starts — the coached side simply
//! goes in without an ability, which is exactly what it did before any of this existed.

use bevy::prelude::*;
use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::network::{CreationArtifacts, NetworkRole};
use crate::phase::AppPhase;
use cube_soccer::entities::CubePlayer;
use cube_soccer::game::Team;
use cube_soccer::systems::superpowers::{Superpower, SuperpowerKind};

/// How long to wait for the classifier. The first call loads CLIP, which is not quick.
const FORGE_TIMEOUT_SECS: u64 = 120;

/// What came back from the service.
#[derive(Debug, Clone)]
enum ForgeReply {
    Power {
        kind: SuperpowerKind,
        confidence: f32,
        motif: Option<String>,
    },
    Failed(String),
}

/// The worker channel. One request per session; the reply arrives whenever it arrives.
#[derive(Resource)]
pub struct ForgeRuntime {
    sender: Sender<ForgeReply>,
    receiver: Receiver<ForgeReply>,
    /// The creation session already asked about, so a re-coached round does not ask again.
    asked_for: Option<String>,
}

impl Default for ForgeRuntime {
    fn default() -> Self {
        let (sender, receiver) = unbounded();
        Self {
            sender,
            receiver,
            asked_for: None,
        }
    }
}

/// The power the player drew, once it is known.
#[derive(Resource, Debug, Clone, Default)]
pub struct ForgedPower {
    pub kind: Option<SuperpowerKind>,
    pub confidence: f32,
    /// What the classifier thought it saw — "a lightning bolt". Worth showing; it is the one
    /// piece of evidence that the drawing was actually looked at.
    pub motif: Option<String>,
    pub error: Option<String>,
}

impl ForgedPower {
    /// The badge to show on the HUD: the drawn power's, or none until one is known.
    pub fn badge_path(&self) -> Option<String> {
        self.kind.map(|kind| kind.badge_path())
    }
}

pub struct ForgePlugin;

impl Plugin for ForgePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ForgeRuntime>()
            .init_resource::<ForgedPower>()
            .add_systems(OnEnter(AppPhase::Coaching), ask_what_was_drawn)
            .add_systems(Update, receive_the_answer)
            // Armed as the match starts, and again at every round boundary -- a player re-coached
            // for round two keeps the power they drew.
            .add_systems(OnEnter(AppPhase::Game), arm_the_coached_side);
    }
}

/// Ask the service to name the power in the drawing, once per session.
///
/// Fired on entering coaching rather than on finishing the painting, because that is the first
/// moment the manifest and both PNGs are certainly on disk — and it leaves the whole of the
/// coaching session for the answer to come back in, so nothing waits on it.
fn ask_what_was_drawn(
    mut runtime: ResMut<ForgeRuntime>,
    artifacts: Res<CreationArtifacts>,
    mut forged: ResMut<ForgedPower>,
) {
    let (Some(session_id), Some(directory)) = (&artifacts.session_id, &artifacts.directory) else {
        return;
    };
    if runtime.asked_for.as_deref() == Some(session_id.as_str()) {
        return;
    }

    let drawing = directory.join("superpower.png");
    if !drawing.exists() {
        // Nothing was painted. Not an error worth showing: the side just has no ability.
        return;
    }

    runtime.asked_for = Some(session_id.clone());
    forged.error = None;
    let sender = runtime.sender.clone();

    // Blocking HTTP on a worker thread, the same shape `speech.rs` and `interpretation.rs` use.
    // A blocking call in a Bevy system would stall the frame for as long as CLIP takes to load.
    std::thread::spawn(move || {
        let reply = match std::fs::read(&drawing) {
            Ok(bytes) => classify(&bytes),
            Err(error) => ForgeReply::Failed(format!("Cannot read {}: {error}", drawing.display())),
        };
        let _ = sender.send(reply);
    });
}

/// POST the drawing and read back which power it is.
fn classify(png: &[u8]) -> ForgeReply {
    use base64::Engine;

    // Read fresh each call: a joiner has `TACTIC_LAB_API_URL` pointed at the host the moment it
    // connects, and every other call in this crate honours that the same way.
    let endpoint =
        std::env::var("TACTIC_LAB_API_URL").unwrap_or_else(|_| "http://127.0.0.1:8787".to_owned());

    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(FORGE_TIMEOUT_SECS))
        .build()
    {
        Ok(client) => client,
        Err(error) => return ForgeReply::Failed(format!("Cannot build an HTTP client: {error}")),
    };

    let body = serde_json::json!({
        "drawing": base64::engine::general_purpose::STANDARD.encode(png),
    });

    let response = match client
        .post(format!("{endpoint}/api/forge/power"))
        .json(&body)
        .send()
    {
        Ok(response) => response,
        Err(error) => {
            return ForgeReply::Failed(format!(
                "Cannot reach the forge at {endpoint}. Is the local service running? ({error})"
            ))
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let message = response
            .json::<serde_json::Value>()
            .ok()
            .and_then(|body| {
                body.get("message")
                    .and_then(|m| m.as_str())
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| format!("The forge answered {status}."));
        return ForgeReply::Failed(message);
    }

    let body = match response.json::<serde_json::Value>() {
        Ok(body) => body,
        Err(error) => return ForgeReply::Failed(format!("The forge sent unreadable JSON: {error}")),
    };

    let slug = body.get("power").and_then(|p| p.as_str()).unwrap_or_default();
    match SuperpowerKind::from_slug(slug) {
        Some(kind) => ForgeReply::Power {
            kind,
            confidence: body
                .get("confidence")
                .and_then(|c| c.as_f64())
                .unwrap_or_default() as f32,
            motif: body
                .get("motif")
                .and_then(|m| m.as_str())
                .map(str::to_owned),
        },
        // Refused rather than defaulted: an unknown slug means the classifier and the game's enum
        // have drifted, and silently picking a power would hide that behind a wrong ability.
        None => ForgeReply::Failed(format!(
            "The forge named a power this build does not have: {slug:?}"
        )),
    }
}

fn receive_the_answer(runtime: Res<ForgeRuntime>, mut forged: ResMut<ForgedPower>) {
    while let Ok(reply) = runtime.receiver.try_recv() {
        match reply {
            ForgeReply::Power {
                kind,
                confidence,
                motif,
            } => {
                forged.kind = Some(kind);
                forged.confidence = confidence;
                forged.motif = motif;
                forged.error = None;
                info!(
                    "the drawn superpower is {} ({:.0}% confident)",
                    kind.slug(),
                    confidence * 100.0
                );
            }
            ForgeReply::Failed(message) => {
                forged.error = Some(message.clone());
                warn!("could not forge the drawn superpower: {message}");
            }
        }
    }
}

/// Give the coached side the power that was drawn for it.
///
/// Only that side. The opponent goes in without an ability, which is what every player did before
/// this existed — and in a networked match both sides are coached on their own machine, so each
/// arms itself with its own drawing.
fn arm_the_coached_side(
    mut commands: Commands,
    forged: Res<ForgedPower>,
    role: Option<Res<NetworkRole>>,
    players: Query<(Entity, &CubePlayer)>,
) {
    let Some(kind) = forged.kind else {
        return;
    };
    let coached: Team = role
        .map(|role| role.coached_side().game_team())
        .unwrap_or(Team::Orange);

    for (entity, player) in &players {
        if player.team == coached {
            commands.entity(entity).insert(Superpower::new(kind));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_forged_power_names_its_badge() {
        let forged = ForgedPower {
            kind: Some(SuperpowerKind::FreezeRay),
            ..default()
        };
        assert_eq!(
            forged.badge_path().as_deref(),
            Some("icons/powers/freeze_ray.png")
        );
    }

    #[test]
    fn nothing_drawn_means_no_badge() {
        assert_eq!(ForgedPower::default().badge_path(), None);
        assert!(ForgedPower::default().kind.is_none());
    }

    #[test]
    fn the_runtime_asks_once_per_session() {
        // The guard is the session id, not a bool, so re-coaching between rounds does not
        // re-classify the same drawing -- but a genuinely new session would.
        let mut runtime = ForgeRuntime::default();
        assert!(runtime.asked_for.is_none());
        runtime.asked_for = Some("session-a".to_owned());
        assert_eq!(runtime.asked_for.as_deref(), Some("session-a"));
        assert_ne!(runtime.asked_for.as_deref(), Some("session-b"));
    }
}
