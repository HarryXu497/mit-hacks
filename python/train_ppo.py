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
from stable_baselines3.common.callbacks import EvalCallback, CheckpointCallback, BaseCallback


class ShapingAnnealCallback(BaseCallback):
    """Linearly anneal the env's dense-shaping weight 1.0 -> 0.0 over the first
    `anneal_frac` of training, then hold at 0 (pure goal objective)."""
    def __init__(self, total_timesteps, anneal_frac=0.7):
        super().__init__()
        self.total = max(1, int(total_timesteps))
        self.anneal_frac = max(1e-6, float(anneal_frac))

    def _set(self, w):
        try:
            self.training_env.env_method("set_shaping_weight", float(w))
        except Exception:
            pass

    def _on_training_start(self) -> None:
        self._set(1.0)

    def _on_rollout_start(self) -> None:
        frac = (self.num_timesteps / self.total) / self.anneal_frac
        self._set(max(0.0, 1.0 - frac))

    def _on_step(self) -> bool:
        return True

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
    parser.add_argument("--shaping-anneal-frac", type=float, default=0.7,
                        help="fraction of training over which dense-shaping weight decays 1->0")
    parser.add_argument("--resume", type=str, default=None,
                        help="path to a saved model .zip to resume training from "
                             "(must match the current obs/action shape). Pass the same "
                             "--timesteps/--shaping-anneal-frac as the original run.")
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
    try:
        eval_env.set_shaping_weight(0.0)
    except Exception:
        pass

    # Callbacks
    anneal_cb = ShapingAnnealCallback(args.timesteps, args.shaping_anneal_frac)
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
        anneal_cb,
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

    # Resume from a checkpoint if requested (replaces the fresh model above).
    if args.resume:
        print(f"Resuming from checkpoint: {args.resume}")
        model = PPO.load(args.resume, env=env, tensorboard_log=f"./runs/{run_id}")

    # Train. On resume, keep the global step counter (so TB logs + the shaping
    # anneal schedule continue) instead of restarting at 0.
    print(f"Starting training for {args.timesteps} timesteps...")
    model.learn(
        total_timesteps=args.timesteps,
        callback=callbacks,
        progress_bar=True,
        reset_num_timesteps=not bool(args.resume),
    )

    # Save final model
    model.save("cube_soccer_ppo_final")
    print("Training complete! Model saved to cube_soccer_ppo_final.zip")

    if use_wandb:
        wandb.finish()


if __name__ == "__main__":
    main()
