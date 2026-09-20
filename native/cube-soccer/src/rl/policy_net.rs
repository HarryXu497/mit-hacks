//! A tiny, dependency-light MLP that runs an exported SB3 PPO policy in-process.
//!
//! The policy is exported to JSON by `python/export_policy.py` (see that file for
//! the exact SB3 layout). The network is the deterministic *action mean* head:
//!
//! ```text
//! h1   = tanh(W1 · obs + b1)
//! h2   = tanh(W2 · h1  + b2)
//! mean = W3 · h2 + b3
//! ```
//!
//! torch `Linear.weight` is `(out, in)`, so each `W` here is a `Vec` of `out`
//! rows, each `in` long, and a layer computes `y[i] = Σ_j W[i][j]·x[j] + b[i]`.
//!
//! Two ways to act:
//! - [`PolicyNet::mean`] — the deterministic action mean, clamped to `[-1, 1]`.
//! - [`PolicyNet::sample`] — draw from `N(mean, exp(log_std))`, clamped. The
//!   trained checkpoints are diffuse (`exp(log_std) ≈ 2.5`), so the deterministic
//!   mean looks passive and sampling shows the livelier behaviour that scored.

use std::path::Path;

use serde::Deserialize;

/// Weights of a 2-hidden-layer tanh MLP policy, loaded from `policy.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct PolicyNet {
    pub obs_dim: usize,
    pub act_dim: usize,
    w1: Vec<Vec<f32>>,
    b1: Vec<f32>,
    w2: Vec<Vec<f32>>,
    b2: Vec<f32>,
    w3: Vec<Vec<f32>>,
    b3: Vec<f32>,
    /// Per-action log standard deviation (diagonal Gaussian).
    log_std: Vec<f32>,
}

impl PolicyNet {
    /// Parse a `policy.json` string. Validates that every layer's shapes line up.
    pub fn from_json(json: &str) -> Result<Self, String> {
        let net: PolicyNet = serde_json::from_str(json).map_err(|e| e.to_string())?;
        net.validate()?;
        Ok(net)
    }

    /// The exported policy committed alongside the crate (`assets/policy.json`).
    /// Resolves to an absolute path at compile time, so dependent crates (e.g.
    /// the coaching binary) can load the same weights regardless of CWD.
    pub fn default_asset_path() -> &'static str {
        concat!(env!("CARGO_MANIFEST_DIR"), "/assets/policy.json")
    }

    /// Load and parse a `policy.json` file.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let path = path.as_ref();
        let json = std::fs::read_to_string(path)
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        Self::from_json(&json)
    }

    fn validate(&self) -> Result<(), String> {
        let h1 = self.b1.len();
        let h2 = self.b2.len();
        if self.w1.len() != h1 || self.w1.iter().any(|r| r.len() != self.obs_dim) {
            return Err(format!("w1 must be {h1}x{} (out×in)", self.obs_dim));
        }
        if self.w2.len() != h2 || self.w2.iter().any(|r| r.len() != h1) {
            return Err(format!("w2 must be {h2}x{h1}"));
        }
        if self.w3.len() != self.act_dim
            || self.w3.iter().any(|r| r.len() != h2)
            || self.b3.len() != self.act_dim
            || self.log_std.len() != self.act_dim
        {
            return Err(format!("action head must be {}x{h2}", self.act_dim));
        }
        Ok(())
    }

    /// Deterministic action: the policy mean, clamped to `[-1, 1]`.
    pub fn mean(&self, obs: &[f32]) -> Vec<f32> {
        let mut out = self.mean_raw(obs);
        for v in &mut out {
            *v = v.clamp(-1.0, 1.0);
        }
        out
    }

    /// Stochastic action: sample `N(mean, exp(log_std))`, clamped to `[-1, 1]`.
    /// `randn` supplies standard-normal draws (one per action dimension).
    pub fn sample(&self, obs: &[f32], mut randn: impl FnMut() -> f32) -> Vec<f32> {
        let mut out = self.mean_raw(obs);
        for (i, v) in out.iter_mut().enumerate() {
            let std = self.log_std[i].exp();
            *v = (*v + std * randn()).clamp(-1.0, 1.0);
        }
        out
    }

    /// Unclamped forward pass to the action mean.
    fn mean_raw(&self, obs: &[f32]) -> Vec<f32> {
        debug_assert_eq!(obs.len(), self.obs_dim, "obs must be {}", self.obs_dim);
        let h1 = tanh_layer(&self.w1, &self.b1, obs);
        let h2 = tanh_layer(&self.w2, &self.b2, &h1);
        affine(&self.w3, &self.b3, &h2)
    }
}

/// `y[i] = Σ_j W[i][j]·x[j] + b[i]`.
fn affine(w: &[Vec<f32>], b: &[f32], x: &[f32]) -> Vec<f32> {
    w.iter()
        .zip(b)
        .map(|(row, bias)| row.iter().zip(x).map(|(wij, xj)| wij * xj).sum::<f32>() + bias)
        .collect()
}

fn tanh_layer(w: &[Vec<f32>], b: &[f32], x: &[f32]) -> Vec<f32> {
    let mut y = affine(w, b, x);
    for v in &mut y {
        *v = v.tanh();
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    // A hand-checkable 2->2->2 net: identity-ish weights so we can verify the math.
    fn tiny_json() -> String {
        // w1 = I, b1 = 0 -> h1 = tanh(x)
        // w2 = I, b2 = 0 -> h2 = tanh(h1)
        // w3 = I, b3 = 0 -> mean = h2
        // log_std = [ln 2, ln 2]  -> std = 2
        r#"{
          "obs_dim": 2, "act_dim": 2,
          "w1": [[1,0],[0,1]], "b1": [0,0],
          "w2": [[1,0],[0,1]], "b2": [0,0],
          "w3": [[1,0],[0,1]], "b3": [0,0],
          "log_std": [0.6931472, 0.6931472]
        }"#
        .to_string()
    }

    #[test]
    fn forward_matches_hand_computation() {
        let net = PolicyNet::from_json(&tiny_json()).unwrap();
        let x = [0.5f32, -0.3];
        // mean = tanh(tanh(x)), then clamped (well within [-1,1] here).
        let expect0 = 0.5f32.tanh().tanh();
        let expect1 = (-0.3f32).tanh().tanh();
        let out = net.mean(&x);
        assert!((out[0] - expect0).abs() < 1e-6, "{} vs {}", out[0], expect0);
        assert!((out[1] - expect1).abs() < 1e-6);
    }

    #[test]
    fn mean_is_clamped_to_unit_box() {
        // Large inputs still clamp because the head is linear on tanh outputs (|·|<1),
        // but verify the clamp path explicitly via a big-bias net.
        let json = r#"{
          "obs_dim": 1, "act_dim": 1,
          "w1": [[0.0]], "b1": [0.0],
          "w2": [[0.0]], "b2": [0.0],
          "w3": [[0.0]], "b3": [5.0],
          "log_std": [0.0]
        }"#;
        let net = PolicyNet::from_json(json).unwrap();
        assert_eq!(net.mean(&[0.0])[0], 1.0);
    }

    #[test]
    fn sample_adds_scaled_noise() {
        let net = PolicyNet::from_json(&tiny_json()).unwrap();
        let x = [0.0f32, 0.0];
        // mean at x=0 is 0; a unit normal draw with std=2 lands at +2 -> clamps to 1.
        let out = net.sample(&x, || 1.0);
        assert_eq!(out, vec![1.0, 1.0]);
        // A zero draw reproduces the (clamped) mean.
        let out0 = net.sample(&x, || 0.0);
        assert_eq!(out0, vec![0.0, 0.0]);
    }

    #[test]
    fn rejects_mismatched_shapes() {
        let json = r#"{
          "obs_dim": 3, "act_dim": 1,
          "w1": [[1,0]], "b1": [0],
          "w2": [[1]], "b2": [0],
          "w3": [[1]], "b3": [0],
          "log_std": [0]
        }"#;
        assert!(PolicyNet::from_json(json).is_err(), "w1 row width != obs_dim");
    }

    #[test]
    fn loads_exported_policy_asset_with_expected_dims() {
        // The committed export is the 425->256->256->20 tactics+superpowers policy.
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/policy.json");
        if !std::path::Path::new(path).exists() {
            return; // export not present in this checkout; skip rather than fail.
        }
        let net = PolicyNet::load(path).expect("valid policy.json");
        assert_eq!(net.obs_dim, 5 * crate::game::OBSERVATION_SIZE, "team obs = 5 * per-agent");
        assert_eq!(net.act_dim, 5 * crate::game::ACTION_SIZE, "team action = 5 * per-agent");
    }
}
