#!/usr/bin/env python3
"""
Self-Play training for Cube Soccer 3D.

This script implements self-play training where an agent learns by playing
against previous versions of itself.

Usage:
    python self_play.py
    python self_play.py --timesteps 10000000 --pool-size 10
"""

import argparse
from collections import deque
import numpy as np
from stable_baselines3 import PPO
from stable_baselines3.common.callbacks import BaseCallback

from env import CubeSoccerEnv


class SelfPlayCallback(BaseCallback):
    """
    Callback for self-play training.
    Maintains a pool of past models to play against.
    """

    def __init__(self, pool_size=10, update_freq=50000, verbose=0):
        super().__init__(verbose)
        self.model_pool = deque(maxlen=pool_size)
        self.update_freq = update_freq
        self.steps_since_update = 0

    def _on_step(self):
        self.steps_since_update += 1

        if self.steps_since_update >= self.update_freq:
            # Add current model to pool
            params = self.model.get_parameters()
            self.model_pool.append(params.copy())
            self.steps_since_update = 0
            if self.verbose > 0:
                print(f"Added model to pool. Pool size: {len(self.model_pool)}")

        return True

    def get_opponent(self):
        if len(self.model_pool) == 0:
            return None

        # Random opponent from pool
        idx = np.random.randint(len(self.model_pool))
        return self.model_pool[idx]


class SelfPlayEnv:
    """
    Environment wrapper for self-play.
    Player Orange is trained, Player Blue uses opponent from pool.
    """

    def __init__(self, base_env, self_play_callback, model_class=PPO):
        self.env = base_env
        self.callback = self_play_callback
        self.model_class = model_class
        self.opponent = None
        self._current_obs = None

    def reset(self, **kwargs):
        obs, info = self.env.reset(**kwargs)
        self._current_obs = obs

        # Update opponent from pool
        opponent_params = self.callback.get_opponent()
        if opponent_params is not None:
            if self.opponent is None:
                # Create a dummy model for the opponent
                self.opponent = self.model_class("MlpPolicy", self.env)
            self.opponent.set_parameters(opponent_params)

        # Only return Orange player observation
        return obs[:22], info

    def step(self, action):
        # Get opponent action
        if self.opponent is not None:
            opp_obs = self._current_obs[22:]  # Blue player observation
            opp_action, _ = self.opponent.predict(opp_obs, deterministic=False)
        else:
            opp_action = np.zeros(4, dtype=np.float32)

        # Combine actions
        full_action = np.concatenate([action, opp_action])

        obs, reward, done, truncated, info = self.env.step(full_action)
        self._current_obs = obs

        # Return only Orange player's perspective
        return obs[:22], reward, done, truncated, info

    @property
    def observation_space(self):
        # Only Orange player observation
        from gymnasium import spaces
        return spaces.Box(low=-np.inf, high=np.inf, shape=(22,), dtype=np.float32)

    @property
    def action_space(self):
        # Only Orange player actions
        from gymnasium import spaces
        return spaces.Box(low=-1.0, high=1.0, shape=(4,), dtype=np.float32)


def main():
    parser = argparse.ArgumentParser(description="Self-play training for Cube Soccer")
    parser.add_argument("--timesteps", type=int, default=10_000_000, help="Total training timesteps")
    parser.add_argument("--pool-size", type=int, default=10, help="Size of opponent pool")
    parser.add_argument("--update-freq", type=int, default=50000, help="Frequency to add model to pool")
    args = parser.parse_args()

    # Create base environment
    base_env = CubeSoccerEnv()

    # Create self-play callback
    self_play_callback = SelfPlayCallback(
        pool_size=args.pool_size,
        update_freq=args.update_freq,
        verbose=1
    )

    # Create self-play wrapper
    env = SelfPlayEnv(base_env, self_play_callback)

    # Create model
    model = PPO(
        "MlpPolicy",
        env,
        verbose=1,
        learning_rate=3e-4,
        n_steps=2048,
        batch_size=256,
        policy_kwargs=dict(
            net_arch=dict(pi=[256, 256], vf=[256, 256])
        ),
    )

    # Train with self-play
    print(f"Starting self-play training for {args.timesteps} timesteps...")
    model.learn(
        total_timesteps=args.timesteps,
        callback=self_play_callback,
        progress_bar=True,
    )

    # Save final model
    model.save("cube_soccer_selfplay_final")
    print("Training complete! Model saved to cube_soccer_selfplay_final.zip")


if __name__ == "__main__":
    main()
