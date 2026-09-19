use bevy::prelude::*;

#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
pub enum AppPhase {
    #[default]
    Creation,
    Coaching,
    Game,
}
