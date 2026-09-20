"""Export an SB3 PPO MlpPolicy to a plain-JSON weight file for the Rust game.

The trained policy controls ONE team: obs = PLAYERS_PER_TEAM * OBSERVATION_SIZE
(currently 5 * 85 = 425), action = PLAYERS_PER_TEAM * ACTION_SIZE (5 * 4 = 20).

Architecture (SB3 MlpPolicy, Box action, no VecNormalize):
    mlp_extractor.policy_net.0 : Linear(obs, 256)  -> Tanh
    mlp_extractor.policy_net.2 : Linear(256, 256)  -> Tanh
    action_net                 : Linear(256, action)
    log_std                    : (action,)

Deterministic action = action_net(policy_net(obs)), then clamp to [-1, 1].
Stochastic action     = sample N(mean, exp(log_std)), then clamp.

torch Linear.weight is (out, in) so y = W @ x + b. We dump weights row-major
(list of `out` rows, each `in` long) so Rust can do `sum_j W[i][j]*x[j] + b[i]`.

Usage:
    .venv/bin/python native/cube-soccer/python/export_policy.py <checkpoint.zip> <out.json>
"""

import json
import sys

import numpy as np
from stable_baselines3 import PPO


def _mat(t):
    """(out, in) tensor -> list[list[float]] row-major."""
    return t.detach().cpu().numpy().astype(np.float64).tolist()


def _vec(t):
    return t.detach().cpu().numpy().astype(np.float64).ravel().tolist()


def main():
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(2)
    ckpt, out_path = sys.argv[1], sys.argv[2]

    model = PPO.load(ckpt, device="cpu")
    sd = model.policy.state_dict()

    # Fail loudly if the architecture isn't the expected 2-hidden-layer MLP.
    required = [
        "mlp_extractor.policy_net.0.weight",
        "mlp_extractor.policy_net.0.bias",
        "mlp_extractor.policy_net.2.weight",
        "mlp_extractor.policy_net.2.bias",
        "action_net.weight",
        "action_net.bias",
        "log_std",
    ]
    missing = [k for k in required if k not in sd]
    if missing:
        print("state_dict keys:", list(sd.keys()))
        raise SystemExit(f"Missing expected keys: {missing}")

    w1 = sd["mlp_extractor.policy_net.0.weight"]
    w2 = sd["mlp_extractor.policy_net.2.weight"]
    w3 = sd["action_net.weight"]

    obs_dim = w1.shape[1]
    hidden1 = w1.shape[0]
    hidden2 = w2.shape[0]
    act_dim = w3.shape[0]

    export = {
        "obs_dim": int(obs_dim),
        "act_dim": int(act_dim),
        "hidden": [int(hidden1), int(hidden2)],
        "activation": "tanh",
        # layer i: y = tanh(Wi @ x + bi) for hidden, linear for the action head.
        "w1": _mat(w1),
        "b1": _vec(sd["mlp_extractor.policy_net.0.bias"]),
        "w2": _mat(w2),
        "b2": _vec(sd["mlp_extractor.policy_net.2.bias"]),
        "w3": _mat(w3),
        "b3": _vec(sd["action_net.bias"]),
        "log_std": _vec(sd["log_std"]),
        "source_checkpoint": ckpt,
    }

    with open(out_path, "w") as f:
        json.dump(export, f)

    std = np.exp(np.array(export["log_std"]))
    print(f"exported {ckpt} -> {out_path}")
    print(f"  obs_dim={obs_dim}  hidden={hidden1},{hidden2}  act_dim={act_dim}")
    print(f"  log_std mean exp={std.mean():.3f} (min={std.min():.3f} max={std.max():.3f})")


if __name__ == "__main__":
    main()
