//! Benchmark for Cube Soccer environment
//!
//! Measures steps per second in headless mode.
//!
//! Run with: cargo run --release --example benchmark

use std::time::Instant;
use cube_soccer::{CubeSoccerEnv, EnvConfig};
use cube_soccer::rl::TOTAL_ACTION_SIZE;
use rand::Rng;

fn main() {
    println!("Cube Soccer 3D - Performance Benchmark");
    println!("=======================================\n");

    let config = EnvConfig {
        headless: true,
        ..Default::default()
    };

    let mut env = CubeSoccerEnv::new(config);

    let num_steps = 20_000;
    let mut rng = rand::thread_rng();

    // Generate random actions
    let random_actions: Vec<Vec<f32>> = (0..num_steps)
        .map(|_| (0..TOTAL_ACTION_SIZE).map(|_| rng.gen_range(-1.0..=1.0)).collect())
        .collect();

    println!("Warming up...");
    env.reset(Some(42));

    // Warmup
    for actions in random_actions.iter().take(1000) {
        let result = env.step(actions);
        if result.done || result.truncated {
            env.reset(None);
        }
    }

    println!("Running benchmark with {} steps...\n", num_steps);

    env.reset(Some(42));
    let start = Instant::now();

    let mut resets = 0;
    for actions in &random_actions {
        let result = env.step(actions);
        if result.done || result.truncated {
            env.reset(None);
            resets += 1;
        }
    }

    let elapsed = start.elapsed();
    let steps_per_sec = num_steps as f64 / elapsed.as_secs_f64();

    println!("Benchmark Results:");
    println!("  Total steps:    {}", num_steps);
    println!("  Total resets:   {}", resets);
    println!("  Time:           {:.2?}", elapsed);
    println!("  Steps/sec:      {:.0}", steps_per_sec);
    println!();

    println!("  (headless Bevy+Rapier sim; use this number to decide whether further");
    println!("   optimization -- dropping the ECS and driving Rapier directly -- is worth it.)");
}
