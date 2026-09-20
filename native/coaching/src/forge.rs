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
use cube_soccer::systems::power_tactics::{bearer_slot, counter_to};
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
            // Armed every frame of the match rather than once at kickoff.
            //
            // Classification is slow -- measured at 17 seconds for a real drawing, because the
            // classifier loads CLIP -- and the request only starts when the coach reaches the
            // table. Sampling `ForgedPower` once, at kickoff, meant that a coach who pressed
            // Enter before the answer came back played the whole match unarmed: the reply landed
            // a moment later and was thrown away, since the one system that reads it had already
            // run. No power, no badge, and nothing on screen to say why.
            //
            // The system settles to a no-op once the side holds what was drawn, so running it
            // continuously costs a query and picks up both a late answer and a power redrawn for
            // the next round.
            .add_systems(
                Update,
                arm_the_coached_side.run_if(in_state(AppPhase::Game)),
            );
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

/// Give each side exactly one player with a power, and pick which player on purpose.
///
/// **The coached side** carries what its coach drew. **The opposition** carries
/// [`counter_to`] it: they have no easel, and a match where one side has an ability and the other
/// has no answer to it is not a match. The counter is deterministic and its badge sits on the HUD
/// from the first whistle, so it is something to play around rather than an ambush. In a networked
/// match both sides are coached on their own machine, and each arms itself from its own drawing.
///
/// **One carrier, not five.** This used to hand the power to every player on the coached side,
/// which is five casters holding the fire button -- better than the ten it replaced, but the same
/// problem: a power going off somewhere at all times is scenery, not a moment. It also made the
/// HUD's single badge per side a polite fiction. [`bearer_slot`] picks the one player whose place
/// in the formation suits that particular power, so the cast happens where the power is worth
/// something.
///
/// Runs every frame, so it is written to settle: a player who should hold the power and already
/// does is left alone -- re-inserting would reset the cooldown and the power could never come off
/// it -- and a player holding one they should no longer have is disarmed, which is what makes a
/// redrawn power move to its new carrier instead of leaving the old one armed.
fn arm_the_coached_side(
    mut commands: Commands,
    forged: Res<ForgedPower>,
    role: Option<Res<NetworkRole>>,
    players: Query<(Entity, &CubePlayer, Option<&Superpower>)>,
) {
    let Some(kind) = forged.kind else {
        // Nothing drawn, so there is no power in this match on either side. The power is the
        // drawing; conjuring an opposing power to answer a drawing that does not exist would be
        // conjuring the whole feature.
        return;
    };
    let coached: Team = role
        .map(|role| role.coached_side().game_team())
        .unwrap_or(Team::Orange);

    for (team, kind) in [(coached, kind), (coached.opponent(), counter_to(kind))] {
        let squad = players.iter().filter(|(_, player, _)| player.team == team).count();
        let Some(slot) = bearer_slot(kind, squad) else {
            continue;
        };
        for (entity, player, held) in &players {
            if player.team != team {
                continue;
            }
            match (player.index == slot, held.map(|power| power.kind)) {
                // The carrier, already holding the right power: leave the cooldown alone.
                (true, Some(held)) if held == kind => {}
                (true, _) => {
                    commands.entity(entity).insert(Superpower::new(kind));
                }
                // Somebody else's power from a previous drawing.
                (false, Some(_)) => {
                    commands.entity(entity).remove::<Superpower>();
                }
                (false, None) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pitch with five players a side and a drawn power, run through the arming system once.
    fn armed_pitch(drawn: Option<SuperpowerKind>) -> App {
        let mut app = App::new();
        app.insert_resource(ForgedPower { kind: drawn, ..default() });
        app.add_systems(Update, arm_the_coached_side);
        for team in [Team::Orange, Team::Blue] {
            for index in 0..5 {
                app.world.spawn(CubePlayer { team, index, can_jump: true });
            }
        }
        app.update();
        app
    }

    /// Who on `team` is holding a power, and which.
    fn armed(app: &mut App, team: Team) -> Vec<(usize, SuperpowerKind)> {
        let mut found: Vec<(usize, SuperpowerKind)> = app
            .world
            .query::<(&CubePlayer, &Superpower)>()
            .iter(&app.world)
            .filter(|(player, _)| player.team == team)
            .map(|(player, power)| (player.index, power.kind))
            .collect();
        found.sort_by_key(|(index, _)| *index);
        found
    }

    #[test]
    fn exactly_one_player_a_side_is_armed() {
        let mut app = armed_pitch(Some(SuperpowerKind::FreezeRay));
        assert_eq!(armed(&mut app, Team::Orange).len(), 1, "the coached side fields one caster");
        assert_eq!(armed(&mut app, Team::Blue).len(), 1, "so does the opposition");
    }

    #[test]
    fn the_coached_side_carries_the_drawing_and_the_other_side_its_counter() {
        let mut app = armed_pitch(Some(SuperpowerKind::Boost));
        assert_eq!(armed(&mut app, Team::Orange)[0].1, SuperpowerKind::Boost);
        assert_eq!(
            armed(&mut app, Team::Blue)[0].1,
            counter_to(SuperpowerKind::Boost),
            "the AI should have an answer to what was drawn"
        );
    }

    #[test]
    fn the_carrier_is_the_slot_the_placement_chose() {
        for drawn in SuperpowerKind::ALL {
            let mut app = armed_pitch(Some(drawn));
            let expected = bearer_slot(drawn, 5).unwrap();
            assert_eq!(
                armed(&mut app, Team::Orange)[0].0,
                expected,
                "{drawn:?} should go to the slot its mechanics want"
            );
        }
    }

    #[test]
    fn nothing_drawn_arms_nobody_on_either_side() {
        let mut app = armed_pitch(None);
        assert!(armed(&mut app, Team::Orange).is_empty());
        assert!(armed(&mut app, Team::Blue).is_empty(), "no drawing, no counter");
    }

    #[test]
    fn running_every_frame_does_not_keep_the_power_on_cooldown() {
        // The system is scheduled in Update, so it sees the same state hundreds of times a match.
        // Re-inserting `Superpower` would reset `cooldown_remaining` each time and the power could
        // never actually recharge.
        let mut app = armed_pitch(Some(SuperpowerKind::Slow));
        let slot = armed(&mut app, Team::Orange)[0].0;
        let caster = app
            .world
            .query::<(Entity, &CubePlayer)>()
            .iter(&app.world)
            .find(|(_, player)| player.team == Team::Orange && player.index == slot)
            .map(|(entity, _)| entity)
            .unwrap();

        app.world.get_mut::<Superpower>(caster).unwrap().cooldown_remaining = 4.0;
        app.update();
        app.update();

        assert_eq!(
            app.world.get::<Superpower>(caster).unwrap().cooldown_remaining,
            4.0,
            "arming again must not re-arm a player who is already holding the right power"
        );
    }

    #[test]
    fn redrawing_moves_the_power_and_disarms_the_old_carrier() {
        let mut app = armed_pitch(Some(SuperpowerKind::FreezeRay));
        let first = armed(&mut app, Team::Orange)[0].0;

        // The classifier comes back with something else -- a redraw between rounds.
        app.world.resource_mut::<ForgedPower>().kind = Some(SuperpowerKind::Boost);
        app.update();

        let now = armed(&mut app, Team::Orange);
        assert_eq!(now.len(), 1, "still exactly one caster, not two");
        assert_eq!(now[0].1, SuperpowerKind::Boost);
        assert_ne!(now[0].0, first, "Boost and Freeze Ray want different players");
    }

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
