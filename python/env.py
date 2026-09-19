"""
Gymnasium-compatible wrapper for Cube Soccer 3D environment.
"""

import gymnasium as gym
from gymnasium import spaces
import numpy as np

try:
    from cube_soccer import PyCubeSoccerEnv
except ImportError:
    PyCubeSoccerEnv = None


class CubeSoccerEnv(gym.Env):
    """
    Cube Soccer 3D environment for RL training.

    Observation Space: Box(44,) - 22 features per player
    Action Space: Box(8,) - 4 actions per player (move_x, move_z, jump, unused)

    This is a two-player zero-sum game.

    Controls:
    - Orange player: WASD + Space (keyboard)
    - Blue player: Arrow keys + Enter (keyboard)

    For RL training, use the action space directly.
    """

    metadata = {"render_modes": ["human", "rgb_array"]}

    def __init__(self, render_mode=None, **kwargs):
        super().__init__()

        if PyCubeSoccerEnv is None:
            raise ImportError(
                "cube_soccer native module not found. "
                "Please build with: maturin develop --release"
            )

        self.render_mode = render_mode
        headless = render_mode is None

        self._env = PyCubeSoccerEnv(headless=headless, render_mode=render_mode)

        # Observation: 22 features per player, 2 players
        self.observation_space = spaces.Box(
            low=-np.inf,
            high=np.inf,
            shape=(44,),
            dtype=np.float32
        )

        # Actions: 4 per player (move_x, move_z, jump, unused)
        self.action_space = spaces.Box(
            low=-1.0,
            high=1.0,
            shape=(8,),
            dtype=np.float32
        )

    def reset(self, seed=None, options=None):
        super().reset(seed=seed)
        obs = self._env.reset(seed)
        return np.array(obs, dtype=np.float32), {}

    def step(self, action):
        action = np.asarray(action, dtype=np.float32)
        obs, rewards, done, truncated, info = self._env.step(action)

        # For single-agent training, return Orange player reward
        # For multi-agent, use CubeSoccerMultiAgentEnv
        reward = rewards[0]  # Orange player reward

        return np.array(obs, dtype=np.float32), reward, done, truncated, info

    def render(self):
        if self.render_mode == "human":
            self._env.render()
        return None

    def close(self):
        pass


class CubeSoccerMultiAgentEnv(CubeSoccerEnv):
    """
    Multi-agent version that returns rewards for both players.
    Compatible with PettingZoo-style training.
    """

    def step(self, action):
        action = np.asarray(action, dtype=np.float32)
        obs, rewards, done, truncated, info = self._env.step(action)

        # Split observations
        obs_orange = np.array(obs[:22], dtype=np.float32)
        obs_blue = np.array(obs[22:], dtype=np.float32)

        return {
            "orange": obs_orange,
            "blue": obs_blue,
        }, {
            "orange": rewards[0],
            "blue": rewards[1],
        }, done, truncated, info

    def reset(self, seed=None, options=None):
        obs, info = super().reset(seed=seed, options=options)

        return {
            "orange": obs[:22],
            "blue": obs[22:],
        }, info


# Register with Gymnasium
try:
    gym.register(
        id="CubeSoccer-v0",
        entry_point="cube_soccer:CubeSoccerEnv",
    )
    gym.register(
        id="CubeSoccerMultiAgent-v0",
        entry_point="cube_soccer:CubeSoccerMultiAgentEnv",
    )
except Exception:
    pass  # Already registered or gymnasium not available
