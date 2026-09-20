#!/usr/bin/env python3
"""Tactic-fidelity eval for the tactic-conditioned Cube Soccer policy.

`eval.py` answers "does it still score". This answers the other half: does the
policy actually *behave differently* when you set different tactics? It sets each
shipped preset on Orange, rolls out the DETERMINISTIC (deployed) policy at full
5v5, and measures team statistics that each tactic should move in a known
direction (docs/TACTICS.md §4e):

    avg_x     mean Orange x. Orange attacks +x, so higher = more advanced.
              HighPress (high line/push) > Balanced > LowBlock (deep).
    z_spread  std of Orange z. Wider = more lateral spread. WingPlay highest.
    ball_dist mean Orange distance to the ball (normalized). Lower = collapses on
              the ball harder. HighPress (press 0.7) lowest of the four.

All stats are read straight from the observation (each agent's self pos + ball-
relative block), so no extra native bindings are needed.

Run from native/cube-soccer/:
    PYTHONPATH=python python tactic_fidelity.py                 # newest checkpoint
    PYTHONPATH=python python tactic_fidelity.py --model models/tactic_seed.zip
"""
import argparse
import glob
import os

import numpy as np
from stable_baselines3 import PPO

from env import CubeSoccerTeamEnv

PRESETS = ["Balanced", "High Press", "Low Block", "Wing Play"]
N_EP = 15
ROSTER = 5          # evaluate the deployed full roster
GOAL_HW = 3.8       # regulation mouth
DIFF = 1.0          # full-strength opponent
FIELD_HALF_X = 24.0  # FIELD_WIDTH / 2 (obs self-x normalizer)
FIELD_HALF_Z = 16.0  # FIELD_DEPTH / 2 (obs self-z normalizer)


def team_stats_from_obs(obs, S, roster):
    """Decode per-step team stats for the `roster` active Orange agents from the
    flat team observation. Obs layout per agent: self pos(3), self vel(3),
    teammates 6*(P-1), opponents 6*P, then ball-relative pos(3)+vel(3)."""
    xs, zs, ball_d = [], [], []
    ball_off = 6 + 6 * (5 - 1) + 6 * 5  # = 60: start of the ball-relative block
    for a in range(roster):
        base = a * S
        xs.append(obs[base + 0] * FIELD_HALF_X)
        zs.append(obs[base + 2] * FIELD_HALF_Z)
        brx, bry, brz = obs[base + ball_off], obs[base + ball_off + 1], obs[base + ball_off + 2]
        ball_d.append(float(np.sqrt(brx * brx + bry * bry + brz * brz)))
    return np.mean(xs), np.std(zs), np.mean(ball_d)


def measure(model, preset):
    # A FRESH env per episode: same-env rollouts are not reproducible (Rapier solver
    # + heuristic sticky-handler state persist across reset), so reusing one env would
    # measure residual-state drift, not the tactic. With a fresh env and a shared seed
    # across presets, identical initial conditions mean any divergence is the tactic.
    ax, zs, bd = [], [], []
    for ep in range(N_EP):
        env = CubeSoccerTeamEnv()
        env.set_shaping_weight(0.0)
        env.set_active_roster(ROSTER)
        env.set_goal_half_width(GOAL_HW)
        env.set_opponent_difficulty(DIFF)
        env.set_team_preset("orange", preset)  # before reset -> first obs carries it
        S = env.observation_size
        obs, _ = env.reset(seed=900 + ep)
        done = trunc = False
        while not (done or trunc):
            a, _ = model.predict(obs, deterministic=True)
            obs, _, done, trunc, _ = env.step(a)
            mx, sz, mb = team_stats_from_obs(np.asarray(obs), S, ROSTER)
            ax.append(mx); zs.append(sz); bd.append(mb)
        env.close()
    return np.mean(ax), np.mean(zs), np.mean(bd)


def main():
    ap = argparse.ArgumentParser(description="Measure whether tactics change the policy's behavior")
    ap.add_argument("--model", default=None, help="path to a .zip (default: newest models/checkpoints/*.zip)")
    args = ap.parse_args()

    path = args.model
    if path is None:
        ckpts = glob.glob("models/checkpoints/*.zip")
        if not ckpts:
            raise SystemExit("no checkpoints in models/checkpoints/ — pass --model")
        path = max(ckpts, key=os.path.getmtime)
    print(f"model: {path}\n")

    model = PPO.load(path)

    print(f"{'tactic':<12}{'avg_x':>9}{'z_spread':>10}{'ball_dist':>11}")
    stats = {}
    for preset in PRESETS:
        mx, sz, mb = measure(model, preset)
        stats[preset] = (mx, sz, mb)
        print(f"{preset:<12}{mx:>9.2f}{sz:>10.2f}{mb:>11.3f}")

    # Directional checks: each should hold once the policy has learned to condition.
    print("\nfidelity checks (expected once trained):")
    def check(label, ok):
        print(f"  [{'PASS' if ok else 'FAIL'}] {label}")
    check("HighPress more advanced than LowBlock (avg_x)", stats["High Press"][0] > stats["Low Block"][0])
    check("WingPlay wider than Balanced (z_spread)",       stats["Wing Play"][1] > stats["Balanced"][1])
    check("HighPress collapses on ball more than LowBlock (ball_dist)", stats["High Press"][2] < stats["Low Block"][2])

    spread_x = max(s[0] for s in stats.values()) - min(s[0] for s in stats.values())
    print(f"\navg_x range across tactics: {spread_x:.2f}  "
          f"({'behaviors differ' if spread_x > 1.0 else 'NEARLY IDENTICAL — policy may be ignoring the tactic'})")


if __name__ == "__main__":
    main()
