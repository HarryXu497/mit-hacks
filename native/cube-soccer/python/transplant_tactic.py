#!/usr/bin/env python3
"""
Weight transplant: no-tactic scorer -> tactic-conditioned policy.

The scorer was trained with a 78-dim per-agent observation (team obs = 5*78 = 390).
Tactic-conditioning appends 7 normalized tactic params per agent, so the per-agent
obs becomes 85 and the team obs 425. That changes the policy's INPUT layer shape, so
the scorer zip can't be `PPO.load`ed into the new env directly.

This script builds a fresh 425-input policy with the same architecture, copies every
matching weight from the scorer, and for the first layer of both the policy and value
MLPs it scatters the old per-agent columns into their new offsets and ZEROES the 7 new
tactic columns per agent. Because the tactic columns start at zero, the transplanted
policy is behaviourally identical to the scorer until training teaches it to modulate
by the tactic — it keeps all the motor skills and only learns the new modulation.

CRUCIAL detail: the observation is agent-major, so the 7 new columns are NOT appended
to the end of the 425 vector — they sit inside each agent's block:
    old flat: [a0:0..78][a1:0..78]...          (78 per agent)
    new flat: [a0:0..78][a0 tac:78..85][a1...]  (85 per agent)
So old column (a*78 + f) maps to new column (a*85 + f), and columns a*85+78..a*85+85
are the fresh (zeroed) tactic inputs. A naive "copy first 390, zero last 35" would
corrupt every agent after the first.

Usage:
    PYTHONPATH=python python python/transplant_tactic.py \
        --src models/checkpoints/ppo_cube_soccer_65600000_steps.zip \
        --out models/tactic_seed.zip
"""

import argparse

import numpy as np
import torch
from stable_baselines3 import PPO

from env import CubeSoccerTeamEnv

# Must match train_ppo.py's policy architecture so every non-input layer aligns.
POLICY_KWARGS = dict(net_arch=dict(pi=[256, 256], vf=[256, 256]))

# PPO hyperparameters that PERSIST through PPO.load (so the warm-started tactic run
# trains with the same config as the scorer). ent_coef / learning_rate are overridden
# by train_ppo.py on load, so they're omitted here.
PPO_KWARGS = dict(
    n_steps=2048,
    batch_size=256,
    n_epochs=10,
    gamma=0.99,
    gae_lambda=0.95,
    clip_range=0.2,
)


def _expand_obs(old_obs, players_per_team, old_per, new_per):
    """Lift a flat old (390) observation into the new (425) layout by inserting
    `new_per - old_per` zeros at the end of each agent's block."""
    old_obs = np.asarray(old_obs, dtype=np.float32).ravel()
    new_obs = np.zeros(players_per_team * new_per, dtype=np.float32)
    for a in range(players_per_team):
        new_obs[a * new_per : a * new_per + old_per] = old_obs[a * old_per : (a + 1) * old_per]
    return new_obs


def transplant(src_path, out_path, verify=True):
    # 1) Load the scorer WITHOUT an env (its saved obs space is the old 390).
    print(f"Loading scorer: {src_path}")
    old = PPO.load(src_path, device="cpu")
    old_sd = old.policy.state_dict()
    old_obs = int(np.prod(old.observation_space.shape))

    # 2) Build a fresh policy on the new (425) env.
    env = CubeSoccerTeamEnv()
    P = env.players_per_team
    new_per = env.observation_size            # 85
    new_obs = P * new_per                      # 425
    old_per = old_obs // P                     # 78
    if old_obs != P * old_per:
        raise SystemExit(f"scorer obs {old_obs} is not divisible by players_per_team {P}")
    print(f"players_per_team={P}  old_per={old_per} (team {old_obs})  new_per={new_per} (team {new_obs})")
    if new_per - old_per != 7:
        raise SystemExit(f"expected +7 tactic params per agent, got +{new_per - old_per}")

    new = PPO("MlpPolicy", env, policy_kwargs=POLICY_KWARGS, device="cpu", **PPO_KWARGS)
    new_sd = new.policy.state_dict()

    # 3) Copy weights; remap the two input-layer matrices.
    result = {}
    remapped, copied, kept = [], 0, []
    for k, new_w in new_sd.items():
        if k not in old_sd:
            result[k] = new_w
            kept.append(k)
            continue
        old_w = old_sd[k]
        if old_w.shape == new_w.shape:
            result[k] = old_w.clone()
            copied += 1
        elif (old_w.dim() == 2 and old_w.shape[0] == new_w.shape[0]
              and old_w.shape[1] == old_obs and new_w.shape[1] == new_obs):
            # First layer: scatter per-agent columns, leave the tactic columns zero.
            W = torch.zeros_like(new_w)
            for a in range(P):
                W[:, a * new_per : a * new_per + old_per] = old_w[:, a * old_per : (a + 1) * old_per]
            result[k] = W
            remapped.append(k)
        else:
            raise SystemExit(
                f"unexpected shape change for {k}: {tuple(old_w.shape)} -> {tuple(new_w.shape)}. "
                "Architecture must match the scorer (only the obs input dim should differ)."
            )

    if not remapped:
        raise SystemExit("no input-layer remap happened — obs shape may not have changed as expected")
    print(f"remapped input layers: {remapped}")
    print(f"copied {copied} tensors verbatim; kept {len(kept)} fresh: {kept or '[]'}")

    new.policy.load_state_dict(result)

    # 4) Verify: with tactic columns = 0, the transplant must reproduce the scorer's
    #    deterministic action on the same underlying state.
    if verify:
        rng = np.random.default_rng(0)
        max_diff = 0.0
        for _ in range(64):
            o_old = rng.standard_normal(old_obs).astype(np.float32)
            o_new = _expand_obs(o_old, P, old_per, new_per)
            a_old, _ = old.predict(o_old, deterministic=True)
            a_new, _ = new.predict(o_new, deterministic=True)
            max_diff = max(max_diff, float(np.max(np.abs(a_old - a_new))))
        print(f"verification: max |action_old - action_new| over 64 states = {max_diff:.3e}")
        if max_diff > 1e-4:
            raise SystemExit("transplant changed the zero-tactic policy — remap is wrong")
        print("verified: zero-tactic behaviour matches the scorer exactly.")

    new.save(out_path)
    print(f"Saved tactic-seed policy to: {out_path}")


def main():
    ap = argparse.ArgumentParser(description="Transplant a no-tactic scorer into a tactic-conditioned policy")
    ap.add_argument("--src", required=True, help="path to the scorer .zip (390-input)")
    ap.add_argument("--out", default="models/tactic_seed.zip", help="output .zip path")
    ap.add_argument("--no-verify", action="store_true", help="skip the behaviour-equivalence check")
    args = ap.parse_args()
    transplant(args.src, args.out, verify=not args.no_verify)


if __name__ == "__main__":
    main()
