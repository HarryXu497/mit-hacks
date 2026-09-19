#!/usr/bin/env python3
"""
PPO Training script for Cube Soccer 3D.

Usage:
    python train_ppo.py
    python train_ppo.py --timesteps 10000000 --num-envs 16
"""

import argparse
from stable_baselines3 import PPO
from stable_baselines3.common.vec_env import SubprocVecEnv, VecMonitor
from stable_baselines3.common.callbacks import EvalCallback, CheckpointCallback

try:
    import wandb
    from wandb.integration.sb3 import WandbCallback
    WANDB_AVAILABLE = True
except ImportError:
    WANDB_AVAILABLE = False

from env import CubeSoccerTeamEnv


def make_env(seed):
    def _init():
        env = CubeSoccerTeamEnv()  # Orange team brain vs built-in heuristic (Blue)
        env.reset(seed=seed)
        return env
    return _init


def main():
    parser = argparse.ArgumentParser(description="Train PPO agent for Cube Soccer")
    parser.add_argument("--timesteps", type=int, default=10_000_000, help="Total training timesteps")
    parser.add_argument("--num-envs", type=int, default=16, help="Number of parallel environments")
    parser.add_argument("--eval-freq", type=int, default=10000, help="Evaluation frequency")
    parser.add_argument("--save-freq", type=int, default=50000, help="Checkpoint save frequency")
    parser.add_argument("--no-wandb", action="store_true", help="Disable wandb logging")
    parser.add_argument("--render-eval", action="store_true", help="Render during evaluation")
    args = parser.parse_args()

    # Wandb logging
    use_wandb = WANDB_AVAILABLE and not args.no_wandb
    if use_wandb:
        run = wandb.init(
            project="cube-soccer",
            sync_tensorboard=True,
            config={
                "timesteps": args.timesteps,
                "num_envs": args.num_envs,
            }
        )
        run_id = run.id
    else:
        run_id = "local"

    # Vectorized environment
    env = SubprocVecEnv([make_env(i) for i in range(args.num_envs)])
    env = VecMonitor(env)

    # Eval environment
    eval_render_mode = "human" if args.render_eval else None
    eval_env = CubeSoccerTeamEnv(render_mode=eval_render_mode)

    # Callbacks
    callbacks = [
        EvalCallback(
            eval_env,
            best_model_save_path="./models/best",
            eval_freq=args.eval_freq,
            n_eval_episodes=10,
            deterministic=True,
        ),
        CheckpointCallback(
            save_freq=args.save_freq,
            save_path="./models/checkpoints",
            name_prefix="ppo_cube_soccer",
        ),
    ]

    if use_wandb:
        callbacks.append(WandbCallback(
            model_save_path=f"./models/wandb/{run_id}",
            verbose=2,
        ))

    # Model
    model = PPO(
        "MlpPolicy",
        env,
        verbose=1,
        learning_rate=3e-4,
        n_steps=2048,
        batch_size=256,
        n_epochs=10,
        gamma=0.99,
        gae_lambda=0.95,
        clip_range=0.2,
        ent_coef=0.01,
        policy_kwargs=dict(
            net_arch=dict(pi=[256, 256], vf=[256, 256])
        ),
        tensorboard_log=f"./runs/{run_id}",
    )

    # Train
    print(f"Starting training for {args.timesteps} timesteps...")
    model.learn(
        total_timesteps=args.timesteps,
        callback=callbacks,
        progress_bar=True,
    )

    # Save final model
    model.save("cube_soccer_ppo_final")
    print("Training complete! Model saved to cube_soccer_ppo_final.zip")

    if use_wandb:
        wandb.finish()


if __name__ == "__main__":
    main()
