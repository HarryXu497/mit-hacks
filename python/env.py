"""
Gymnasium-compatible wrapper for Cube Soccer 3D environment.

NOTE: the native CubeSoccerEnv is currently a stub that returns correctly-shaped
zeros; wiring the headless Bevy simulation is a separate follow-up. The shapes,
per-agent split, and API here are the real ones.
"""

import gymnasium as gym
from gymnasium import spaces
import numpy as np

try:
    from cube_soccer import PyCubeSoccerEnv
except ImportError:
    PyCubeSoccerEnv = None


def _agent_ids(num_agents, players_per_team):
    ids = []
    for team in ("orange", "blue"):
        for i in range(players_per_team):
            ids.append(f"{team}_{i}")
    return ids[:num_agents]


class CubeSoccerEnv(gym.Env):
    """
    Cube Soccer 3D environment.

    Observation Space: Box(num_agents * observation_size,)
    Action Space: Box(num_agents * action_size,)

    Single-agent convenience: `step` returns the Orange-0 agent's reward.
    For per-agent dicts use `CubeSoccerMultiAgentEnv`.
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

        self.num_agents = self._env.num_agents
        self.players_per_team = self._env.players_per_team
        self.observation_size = self._env.observation_size
        self.action_size = self._env.action_size
        self.agent_ids = _agent_ids(self.num_agents, self.players_per_team)

        obs_dim = self.num_agents * self.observation_size
        act_dim = self.num_agents * self.action_size

        self.observation_space = spaces.Box(
            low=-np.inf, high=np.inf, shape=(obs_dim,), dtype=np.float32
        )
        self.action_space = spaces.Box(
            low=-1.0, high=1.0, shape=(act_dim,), dtype=np.float32
        )

    def reset(self, seed=None, options=None):
        super().reset(seed=seed)
        obs = self._env.reset(seed)
        return np.array(obs, dtype=np.float32), {}

    def step(self, action):
        action = np.asarray(action, dtype=np.float32).ravel()
        obs, rewards, done, truncated, info = self._env.step(action)
        reward = float(rewards[0])  # Orange-0 reward
        return np.array(obs, dtype=np.float32), reward, done, truncated, info

    def render(self):
        if self.render_mode == "human":
            self._env.render()
        return None

    def set_shaping_weight(self, w):
        self._env.set_shaping_weight(float(w))

    def set_opponent_difficulty(self, d):
        self._env.set_opponent_difficulty(float(d))

    def close(self):
        pass


class CubeSoccerMultiAgentEnv(CubeSoccerEnv):
    """Per-agent version returning dicts keyed by agent id (e.g. 'orange_0')."""

    def _split_obs(self, obs):
        obs = np.asarray(obs, dtype=np.float32)
        out = {}
        for a, aid in enumerate(self.agent_ids):
            start = a * self.observation_size
            out[aid] = obs[start:start + self.observation_size]
        return out

    def reset(self, seed=None, options=None):
        obs, info = super().reset(seed=seed, options=options)
        return self._split_obs(obs), info

    def step(self, action):
        # Accept a dict of per-agent actions or a flat array.
        if isinstance(action, dict):
            flat = np.zeros(self.num_agents * self.action_size, dtype=np.float32)
            for a, aid in enumerate(self.agent_ids):
                start = a * self.action_size
                flat[start:start + self.action_size] = np.asarray(action[aid], dtype=np.float32)
            action = flat
        else:
            action = np.asarray(action, dtype=np.float32).ravel()

        obs, rewards, done, truncated, info = self._env.step(action)
        rewards_dict = {aid: float(rewards[a]) for a, aid in enumerate(self.agent_ids)}
        return self._split_obs(obs), rewards_dict, done, truncated, info


class CubeSoccerTeamEnv(gym.Env):
    """Single-policy 'team brain' for Orange vs the built-in heuristic (Blue).

    One PPO policy controls all Orange cubes:
    - Observation: the Orange agents' per-agent observations concatenated
      (players_per_team * observation_size).
    - Action: the Orange agents' actions concatenated
      (players_per_team * action_size). Blue's action slice is left zero; the
      native sim drives Blue with the built-in heuristic AI, so it is ignored.
    - Reward: the sum of the Orange agents' per-agent rewards (shared team reward).

    Orange occupies flat agent indices [0, players_per_team); its actions occupy
    the first `players_per_team * action_size` entries of the native action
    vector, and its observations the first block of the native observation vector.
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
        self._env = PyCubeSoccerEnv(headless=render_mode is None, render_mode=render_mode)

        self.num_agents = self._env.num_agents
        self.players_per_team = self._env.players_per_team
        self.observation_size = self._env.observation_size
        self.action_size = self._env.action_size

        self._team_obs_dim = self.players_per_team * self.observation_size
        self._team_act_dim = self.players_per_team * self.action_size
        self._native_act_dim = self.num_agents * self.action_size

        self.observation_space = spaces.Box(
            low=-np.inf, high=np.inf, shape=(self._team_obs_dim,), dtype=np.float32
        )
        self.action_space = spaces.Box(
            low=-1.0, high=1.0, shape=(self._team_act_dim,), dtype=np.float32
        )

    def _orange_obs(self, obs):
        # Orange agents are the first `players_per_team` in the flat obs vector.
        return np.asarray(obs[: self._team_obs_dim], dtype=np.float32)

    def reset(self, seed=None, options=None):
        super().reset(seed=seed)
        obs = self._env.reset(seed)
        return self._orange_obs(obs), {}

    def step(self, action):
        action = np.asarray(action, dtype=np.float32).ravel()
        full = np.zeros(self._native_act_dim, dtype=np.float32)
        full[: self._team_act_dim] = action[: self._team_act_dim]  # Orange; Blue stays 0
        obs, rewards, done, truncated, info = self._env.step(full)
        team_reward = float(np.sum(rewards[: self.players_per_team]))
        return self._orange_obs(obs), team_reward, done, truncated, info

    def set_shaping_weight(self, w):
        self._env.set_shaping_weight(float(w))

    def set_opponent_difficulty(self, d):
        self._env.set_opponent_difficulty(float(d))

    def render(self):
        if self.render_mode == "human":
            self._env.render()
        return None

    def close(self):
        pass


try:
    gym.register(id="CubeSoccer-v0", entry_point="cube_soccer:CubeSoccerEnv")
    gym.register(id="CubeSoccerMultiAgent-v0", entry_point="cube_soccer:CubeSoccerMultiAgentEnv")
    gym.register(id="CubeSoccerTeam-v0", entry_point="cube_soccer:CubeSoccerTeamEnv")
except Exception:
    pass
