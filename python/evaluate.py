#!/usr/bin/env python3
"""
Evaluation script for trained Cube Soccer agents.

Usage:
    python evaluate.py --model cube_soccer_ppo_final.zip
    python evaluate.py --model cube_soccer_ppo_final.zip --episodes 100 --render
"""

import argparse
import numpy as np
from stable_baselines3 import PPO

from env import CubeSoccerEnv


def evaluate(model_path, num_episodes=100, render=False, deterministic=True):
    """Evaluate a trained model."""

    # Load model
    model = PPO.load(model_path)

    # Create environment
    render_mode = "human" if render else None
    env = CubeSoccerEnv(render_mode=render_mode)

    # Statistics
    episode_rewards = []
    episode_lengths = []
    wins = 0
    losses = 0
    draws = 0

    for episode in range(num_episodes):
        obs, info = env.reset()
        episode_reward = 0
        episode_length = 0
        done = False
        truncated = False

        while not (done or truncated):
            # For single-agent evaluation, we only control Orange player
            # Blue player gets zero actions (or random)
            orange_action, _ = model.predict(obs[:22], deterministic=deterministic)
            blue_action = np.zeros(4, dtype=np.float32)  # No-op for blue

            action = np.concatenate([orange_action, blue_action])
            obs, reward, done, truncated, info = env.step(action)

            episode_reward += reward
            episode_length += 1

            if render:
                env.render()

        episode_rewards.append(episode_reward)
        episode_lengths.append(episode_length)

        # Determine outcome
        if "winner" in info:
            if info["winner"] == "Orange":
                wins += 1
            elif info["winner"] == "Blue":
                losses += 1
            else:
                draws += 1
        else:
            # Determine by score
            if info.get("score_orange", 0) > info.get("score_blue", 0):
                wins += 1
            elif info.get("score_orange", 0) < info.get("score_blue", 0):
                losses += 1
            else:
                draws += 1

        if (episode + 1) % 10 == 0:
            print(f"Episode {episode + 1}/{num_episodes}: "
                  f"Reward={episode_reward:.2f}, Length={episode_length}")

    env.close()

    # Print summary
    print("\n" + "=" * 50)
    print("EVALUATION SUMMARY")
    print("=" * 50)
    print(f"Episodes: {num_episodes}")
    print(f"Mean Reward: {np.mean(episode_rewards):.2f} +/- {np.std(episode_rewards):.2f}")
    print(f"Mean Length: {np.mean(episode_lengths):.2f}")
    print(f"Wins: {wins} ({100 * wins / num_episodes:.1f}%)")
    print(f"Losses: {losses} ({100 * losses / num_episodes:.1f}%)")
    print(f"Draws: {draws} ({100 * draws / num_episodes:.1f}%)")
    print("=" * 50)

    return {
        "mean_reward": np.mean(episode_rewards),
        "std_reward": np.std(episode_rewards),
        "mean_length": np.mean(episode_lengths),
        "win_rate": wins / num_episodes,
        "loss_rate": losses / num_episodes,
        "draw_rate": draws / num_episodes,
    }


def main():
    parser = argparse.ArgumentParser(description="Evaluate trained Cube Soccer agent")
    parser.add_argument("--model", type=str, required=True, help="Path to trained model")
    parser.add_argument("--episodes", type=int, default=100, help="Number of evaluation episodes")
    parser.add_argument("--render", action="store_true", help="Render during evaluation")
    parser.add_argument("--stochastic", action="store_true", help="Use stochastic policy")
    args = parser.parse_args()

    evaluate(
        model_path=args.model,
        num_episodes=args.episodes,
        render=args.render,
        deterministic=not args.stochastic,
    )


if __name__ == "__main__":
    main()
