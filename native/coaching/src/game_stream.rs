//! Host-authoritative game streaming (multiplayer Phase C): the host
//! publishes a compact per-tick snapshot of the live match over a direct
//! Rust↔Rust WebSocket; the joiner applies it instead of running its own
//! physics/AI, which is what avoids cross-machine simulation drift.
use crate::network::{NetworkEndpoint, NetworkRole};
use bevy::prelude::*;
use crossbeam_channel::{unbounded, Receiver};
use cube_soccer::entities::{Ball, CubePlayer};
use cube_soccer::game::{GameState, Team};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio_tungstenite::tungstenite::Message;

pub const GAME_STREAM_PORT: u16 = 9010;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlayerSnapshot {
    pub team: u8, // 0 = Orange/red, 1 = Blue/yellow
    pub index: usize,
    pub position: [f32; 3],
    pub yaw: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct GameSnapshot {
    pub tick: u64,
    pub players: Vec<PlayerSnapshot>,
    pub ball_position: [f32; 3],
    pub score: [u32; 2],
    pub time_remaining: f32,
}

/// Host: publishes snapshots via a tokio mpsc sender (cheap, non-blocking,
/// callable directly from a synchronous Bevy system).
/// Joiner: receives snapshots via a crossbeam receiver, polled per frame.
#[derive(Resource)]
pub struct GameStreamRuntime {
    outgoing: Option<UnboundedSender<GameSnapshot>>,
    incoming: Option<Receiver<GameSnapshot>>,
}

impl GameStreamRuntime {
    pub fn host(port: u16) -> Self {
        let (tx, rx) = unbounded_channel::<GameSnapshot>();
        std::thread::Builder::new()
            .name("tactic-lab-game-stream-host".into())
            .spawn(move || host_worker(port, rx))
            .expect("game stream host thread");
        Self {
            outgoing: Some(tx),
            incoming: None,
        }
    }

    pub fn joiner(host_ip: String, port: u16) -> Self {
        let (tx, rx) = unbounded();
        std::thread::Builder::new()
            .name("tactic-lab-game-stream-joiner".into())
            .spawn(move || joiner_worker(host_ip, port, tx))
            .expect("game stream joiner thread");
        Self {
            outgoing: None,
            incoming: Some(rx),
        }
    }

    pub fn publish(&self, snapshot: GameSnapshot) {
        if let Some(sender) = &self.outgoing {
            let _ = sender.send(snapshot);
        }
    }

    /// Drains all buffered snapshots and returns only the freshest one —
    /// there is no point rendering stale intermediate frames.
    pub fn latest(&self) -> Option<GameSnapshot> {
        let receiver = self.incoming.as_ref()?;
        let mut latest = None;
        while let Ok(snapshot) = receiver.try_recv() {
            latest = Some(snapshot);
        }
        latest
    }
}

fn host_worker(port: u16, mut snapshots: UnboundedReceiver<GameSnapshot>) {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };
    runtime.block_on(async move {
        let (broadcast_tx, _) = tokio::sync::broadcast::channel::<String>(64);
        let listener = match tokio::net::TcpListener::bind(("0.0.0.0", port)).await {
            Ok(listener) => listener,
            Err(error) => {
                eprintln!("game stream: failed to bind 0.0.0.0:{port}: {error}");
                return;
            }
        };
        let accept_broadcast = broadcast_tx.clone();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _addr)) = listener.accept().await else {
                    continue;
                };
                let mut receiver = accept_broadcast.subscribe();
                tokio::spawn(async move {
                    let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
                        return;
                    };
                    let (mut write, _read) = ws.split();
                    while let Ok(message) = receiver.recv().await {
                        if write.send(Message::Text(message.into())).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });
        while let Some(snapshot) = snapshots.recv().await {
            if let Ok(json) = serde_json::to_string(&snapshot) {
                let _ = broadcast_tx.send(json);
            }
        }
    });
}

fn joiner_worker(host_ip: String, port: u16, out: crossbeam_channel::Sender<GameSnapshot>) {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };
    runtime.block_on(async move {
        loop {
            let url = format!("ws://{host_ip}:{port}/");
            if let Ok((socket, _)) = tokio_tungstenite::connect_async(&url).await {
                let (_write, mut read) = socket.split();
                while let Some(Ok(message)) = read.next().await {
                    if let Message::Text(text) = message {
                        if let Ok(snapshot) = serde_json::from_str::<GameSnapshot>(&text) {
                            let _ = out.send(snapshot);
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });
}

#[derive(Resource)]
pub struct SnapshotTimer(pub Timer);

impl Default for SnapshotTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(1.0 / 20.0, TimerMode::Repeating))
    }
}

pub fn start_game_stream(
    mut commands: Commands,
    role: Option<Res<NetworkRole>>,
    endpoint: Option<Res<NetworkEndpoint>>,
) {
    match role.as_deref() {
        Some(NetworkRole::Host) => {
            commands.insert_resource(GameStreamRuntime::host(GAME_STREAM_PORT));
        }
        Some(NetworkRole::Joiner) => {
            let host_ip = endpoint
                .map(|endpoint| endpoint.host_only().to_owned())
                .unwrap_or_else(|| "127.0.0.1".into());
            commands.insert_resource(GameStreamRuntime::joiner(host_ip, GAME_STREAM_PORT));
        }
        _ => {}
    }
}

pub fn publish_game_snapshot(
    role: Option<Res<NetworkRole>>,
    runtime: Option<Res<GameStreamRuntime>>,
    mut timer: ResMut<SnapshotTimer>,
    time: Res<Time>,
    mut tick: Local<u64>,
    players: Query<(&CubePlayer, &Transform)>,
    ball: Query<&Transform, With<Ball>>,
    game_state: Res<GameState>,
) {
    let Some(role) = role else { return };
    if !matches!(*role, NetworkRole::Host) {
        return;
    }
    let Some(runtime) = runtime else { return };
    timer.0.tick(time.delta());
    if !timer.0.finished() {
        return;
    }
    *tick += 1;
    let mut snapshot = GameSnapshot {
        tick: *tick,
        players: Vec::with_capacity(10),
        ball_position: [0.0; 3],
        score: game_state.score,
        time_remaining: game_state.time_remaining,
    };
    for (player, transform) in &players {
        snapshot.players.push(PlayerSnapshot {
            team: if player.team == Team::Orange { 0 } else { 1 },
            index: player.index,
            position: transform.translation.to_array(),
            yaw: transform.rotation.to_euler(EulerRot::YXZ).0,
        });
    }
    if let Ok(transform) = ball.get_single() {
        snapshot.ball_position = transform.translation.to_array();
    }
    runtime.publish(snapshot);
}

pub fn apply_network_snapshot(
    role: Option<Res<NetworkRole>>,
    runtime: Option<Res<GameStreamRuntime>>,
    mut players: Query<(&CubePlayer, &mut Transform), Without<Ball>>,
    mut ball: Query<&mut Transform, With<Ball>>,
    mut game_state: ResMut<GameState>,
) {
    let Some(role) = role else { return };
    if !role.is_joiner() {
        return;
    }
    let Some(runtime) = runtime else { return };
    let Some(snapshot) = runtime.latest() else {
        return;
    };
    for player_snapshot in &snapshot.players {
        let team = if player_snapshot.team == 0 {
            Team::Orange
        } else {
            Team::Blue
        };
        for (player, mut transform) in &mut players {
            if player.team == team && player.index == player_snapshot.index {
                transform.translation = Vec3::from(player_snapshot.position);
                transform.rotation = Quat::from_rotation_y(player_snapshot.yaw);
                break;
            }
        }
    }
    if let Ok(mut transform) = ball.get_single_mut() {
        transform.translation = Vec3::from(snapshot.ball_position);
    }
    game_state.score = snapshot.score;
    game_state.time_remaining = snapshot.time_remaining;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// Runs the real host WS server and the real joiner WS client against
    /// each other over an actual loopback TCP socket — the same code path
    /// used when a host and joiner talk over LAN wifi, just on 127.0.0.1
    /// instead of a routed network address.
    #[test]
    fn joiner_receives_snapshots_published_by_a_real_host_over_loopback() {
        let port = 20_000 + (std::process::id() % 5000) as u16;
        let host = GameStreamRuntime::host(port);
        let joiner = GameStreamRuntime::joiner("127.0.0.1".into(), port);

        // Give the host's TCP listener a moment to bind before the joiner
        // starts retrying its connection.
        std::thread::sleep(Duration::from_millis(200));

        let snapshot = GameSnapshot {
            tick: 7,
            players: vec![PlayerSnapshot {
                team: 0,
                index: 2,
                position: [1.5, 0.5, -3.25],
                yaw: 0.75,
            }],
            ball_position: [0.1, 0.2, 0.3],
            score: [2, 1],
            time_remaining: 42.5,
        };

        // Keep publishing (not just once) since the joiner's connection may
        // not have completed its handshake by the time the first send fires.
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut received = None;
        while Instant::now() < deadline {
            host.publish(snapshot.clone());
            std::thread::sleep(Duration::from_millis(100));
            if let Some(latest) = joiner.latest() {
                received = Some(latest);
                break;
            }
        }

        let received = received.expect("joiner never received a snapshot from the real host over loopback");
        assert_eq!(received.tick, 7);
        assert_eq!(received.score, [2, 1]);
        assert_eq!(received.players.len(), 1);
        assert_eq!(received.players[0].index, 2);
        assert_eq!(received.ball_position, [0.1, 0.2, 0.3]);
    }
}
