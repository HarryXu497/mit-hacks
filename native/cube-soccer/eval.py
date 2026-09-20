#!/usr/bin/env python3
"""Side-eval for the Cube Soccer curriculum policy.

Run from native/cube-soccer/:
    PYTHONPATH=python python eval.py

Loads the newest checkpoint (by mtime, so stale higher-numbered files from old
runs are ignored) and reports scoring across rosters / opponent difficulties,
both stochastic and deterministic. Field size scales with roster automatically,
so we only vary --active-roster.
"""
import glob
import os

import numpy as np
from stable_baselines3 import PPO

from env import CubeSoccerTeamEnv

N_EP = 20
ROSTERS = [1, 2, 3, 5]
DIFFS = [0.4, 1.0]
GOAL_HW = 3.8  # regulation mouth (matches training)


def run(model, env, roster, diff, deterministic, n_ep=N_EP):
    go = gb = scored = 0
    for ep in range(n_ep):
        obs, _ = env.reset(seed=700 + ep)
        env.set_shaping_weight(0.0)          # pure goals, no shaping
        env.set_active_roster(roster)        # roster + field size (scale together)
        env.set_goal_half_width(GOAL_HW)
        env.set_opponent_difficulty(diff)
        done = trunc = False
        info = {}
        while not (done or trunc):
            a, _ = model.predict(obs, deterministic=deterministic)
            obs, r, done, trunc, info = env.step(a)
        go += info["score_orange"]
        gb += info["score_blue"]
        scored += info["score_orange"] > 0
    return go, gb, scored


def main():
    ckpts = glob.glob("models/checkpoints/*.zip")
    if not ckpts:
        raise SystemExit("no checkpoints found in models/checkpoints/")
    ckpt = max(ckpts, key=os.path.getmtime)  # newest by mtime = current run
    print(f"checkpoint: {ckpt}\n")

    model = PPO.load(ckpt)
    env = CubeSoccerTeamEnv()

    print(f"{'setting':<22}{'Orange':>7}{'Blue':>6}{'O-scored%':>11}")
    for roster in ROSTERS:
        for diff in DIFFS:
            for det in (False, True):
                go, gb, sc = run(model, env, roster, diff, det)
                tag = f"{roster}v{roster} d={diff} {'det' if det else 'sto'}"
                print(f"{tag:<22}{go:>7}{gb:>6}{100 * sc / N_EP:>10.0f}%")


if __name__ == "__main__":
    main()
