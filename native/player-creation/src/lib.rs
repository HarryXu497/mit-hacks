//! Native player creation for Tactic Lab.
//!
//! Two sequential drawing phases — appearance, then superpower — for each of
//! the ten fixed players, followed by a per-player review and a handoff to the
//! coaching screen.
//!
//! Layering, deliberately kept apart so a later agent can change one without
//! the others:
//!
//! - [`state`] owns flow and roster. No rendering, no IO.
//! - [`drawing`] owns strokes and rasterization. No Bevy, no egui.
//! - [`input`] turns pointer and keyboard into intent.
//! - [`persistence`] writes artifacts behind the [`persistence::CreationStore`]
//!   trait, which is the seam for a future model handoff.
//! - [`ui`] is layout only.

pub mod drawing;
pub mod input;
pub mod persistence;
pub mod state;
pub mod ui;

use bevy::prelude::*;
use persistence::{CreationStore, LocalFileStore};
use state::{ContinueToCoaching, CreationFlow, PlayerCreationSession};
use std::sync::Arc;

/// The active storage backend. Swapping what goes in here is all it takes to
/// send drawings somewhere other than the local filesystem.
#[derive(Resource, Clone)]
pub struct StoreResource(pub Arc<dyn CreationStore>);

impl Default for StoreResource {
    fn default() -> Self {
        Self(Arc::new(LocalFileStore::default()))
    }
}

/// What the plugin does once the user leaves review.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HandoffBehavior {
    /// Keep drawing a summary screen. Correct when nothing else follows, as in
    /// the standalone binary.
    #[default]
    ShowSummary,
    /// Stop drawing entirely, so another plugin — the coaching screen — owns the
    /// window from the handoff onwards.
    Yield,
}

/// Drives the player-creation screens. Add it to any Bevy app that already
/// installs `EguiPlugin`.
#[derive(Default)]
pub struct PlayerCreationPlugin {
    /// Storage backend; defaults to `output/player-creations/`.
    pub store: Option<Arc<dyn CreationStore>>,
    /// Whether to release the window after `ContinueToCoaching`.
    pub handoff: HandoffBehavior,
}

impl PlayerCreationPlugin {
    pub fn with_store(store: Arc<dyn CreationStore>) -> Self {
        Self {
            store: Some(store),
            handoff: HandoffBehavior::default(),
        }
    }

    /// Releases the window once creation finishes, for the combined app.
    pub fn yielding_to_coaching(mut self) -> Self {
        self.handoff = HandoffBehavior::Yield;
        self
    }
}

/// False once creation has handed off and the plugin was asked to yield, so the
/// two screens never draw over each other.
fn creation_ui_should_run(handoff: Res<HandoffBehavior>, flow: Res<State<CreationFlow>>) -> bool {
    !(*handoff == HandoffBehavior::Yield && *flow.get() == CreationFlow::ContinueToCoaching)
}

impl Plugin for PlayerCreationPlugin {
    fn build(&self, app: &mut App) {
        let store = self.store.clone().map(StoreResource).unwrap_or_default();

        app.insert_resource(store)
            .insert_resource(self.handoff)
            .init_state::<CreationFlow>()
            .init_resource::<PlayerCreationSession>()
            .init_resource::<input::ToolSettings>()
            .init_resource::<ui::CreationUiState>()
            .add_event::<ContinueToCoaching>()
            .add_systems(Startup, ui::configure_egui)
            .add_systems(Update, ui::creation_ui.run_if(creation_ui_should_run));
    }
}

/// True when the combined flow is requested via `TACTIC_LAB_FLOW=full`.
///
/// The coaching app reads this to decide whether to start in player creation.
/// See `docs/native-player-creation.md` for the wiring snippet.
pub fn full_flow_requested() -> bool {
    std::env::var("TACTIC_LAB_FLOW")
        .map(|value| value.eq_ignore_ascii_case("full"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flow_starts_on_the_appearance_phase() {
        assert_eq!(CreationFlow::default(), CreationFlow::AppearanceDrawing);
    }

    #[test]
    fn the_plugin_only_yields_the_window_when_asked() {
        assert_eq!(
            PlayerCreationPlugin::default().handoff,
            HandoffBehavior::ShowSummary,
            "standalone keeps drawing its summary screen"
        );
        assert_eq!(
            PlayerCreationPlugin::default()
                .yielding_to_coaching()
                .handoff,
            HandoffBehavior::Yield,
        );
    }

    #[test]
    fn full_flow_is_opt_in() {
        // Absent or unset means the coaching app keeps its current behaviour.
        std::env::remove_var("TACTIC_LAB_FLOW");
        assert!(!full_flow_requested());
        std::env::set_var("TACTIC_LAB_FLOW", "coaching");
        assert!(!full_flow_requested());
        std::env::set_var("TACTIC_LAB_FLOW", "Full");
        assert!(full_flow_requested());
        std::env::remove_var("TACTIC_LAB_FLOW");
    }
}
