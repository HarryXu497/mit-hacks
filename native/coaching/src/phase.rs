use bevy::prelude::*;

#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
pub enum AppPhase {
    #[default]
    Lobby,
    Creation,
    Coaching,
    /// Networked play only: this machine has handed its match entry to the
    /// host and is waiting for the other coach to finish. Without a phase of
    /// its own there is no UI here and the window renders nothing.
    Waiting,
    Game,
}
