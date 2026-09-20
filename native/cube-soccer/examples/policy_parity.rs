//! Parity harness: print the Rust PolicyNet's deterministic action for a fixed,
//! reproducible observation so it can be compared against SB3's own output.
//! Not a game — just a numerical check that the export/forward pass is correct.

use cube_soccer::rl::PolicyNet;

fn main() {
    let path = format!("{}/assets/policy.json", env!("CARGO_MANIFEST_DIR"));
    let net = PolicyNet::load(&path).expect("load policy.json");
    let obs: Vec<f32> = (0..net.obs_dim)
        .map(|i| ((i % 13) as f32 - 6.0) / 10.0)
        .collect();
    let action = net.mean(&obs);
    let parts: Vec<String> = action.iter().map(|v| format!("{v:.6}")).collect();
    println!("{}", parts.join(","));
}
