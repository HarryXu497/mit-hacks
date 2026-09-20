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


class EntropyAnnealCallback(BaseCallback):
    """Decay `ent_coef` from `start` to `end` over the first `anneal_frac` of
    training, then hold. Keyed on absolute `num_timesteps`, so it resumes correctly
    (picks up wherever the step counter is). Sets `model.ent_coef` each rollout —
    keeps exploration adequate early, then sharpens the policy (lowers std) as the
    curriculum reaches the harder, precision-demanding scales."""
    def __init__(self, total_timesteps, start_coef, end_coef, anneal_frac=0.5):
        super().__init__()
        self.total = max(1, int(total_timesteps))
        self.start = float(start_coef)
        self.end = float(end_coef)
        self.frac = max(1e-6, float(anneal_frac))

    def _coef(self):
        p = min(1.0, (self.num_timesteps / self.total) / self.frac)
        return self.start + (self.end - self.start) * p

    def _apply(self):
        try:
            self.model.ent_coef = self._coef()
        except Exception:
            pass

    def _on_training_start(self) -> None:
        self._apply()

    def _on_rollout_start(self) -> None:
        self._apply()

    def _on_step(self) -> bool:
        return True


class OpponentCurriculumCallback(BaseCallback):
    """Ramp the heuristic opponent's difficulty from `floor` -> 1.0 over the first
    `curriculum_frac` of training, then hold at full strength. Starts Blue weak so
    the RL team can actually score and anchor its policy on real goals, then hardens
    the opponent back to the real heuristic."""
    def __init__(self, total_timesteps, curriculum_frac=0.45, floor=0.15):
        super().__init__()
        self.total = max(1, int(total_timesteps))
        self.frac = max(1e-6, float(curriculum_frac))
        self.floor = float(floor)

    def _set(self, d):
        try:
            self.training_env.env_method("set_opponent_difficulty", float(d))
        except Exception:
            pass

    def _on_training_start(self) -> None:
        self._set(self.floor)

    def _on_rollout_start(self) -> None:
        progress = (self.num_timesteps / self.total) / self.frac
        d = self.floor + (1.0 - self.floor) * min(1.0, progress)
        self._set(d)

    def _on_step(self) -> bool:
        return True


class RosterCurriculumCallback(BaseCallback):
    """Grow the active roster from 1v1 to full NvN over the first `curriculum_frac`
    of training. Both teams gain a player at the same time. The obs/action shape is
    fixed at the full roster (benched players are ghosted+frozen in the sim), so a
    single policy trains continuously across roster sizes — 1v1 is learnable, and
    each added player is a small step up."""
    def __init__(self, total_timesteps, players_per_team, curriculum_frac=0.5):
        super().__init__()
        self.total = max(1, int(total_timesteps))
        self.max_n = max(1, int(players_per_team))
        self.frac = max(1e-6, float(curriculum_frac))

    def _active(self):
        prog = (self.num_timesteps / self.total) / self.frac
        n = 1 + int(min(1.0, prog) * (self.max_n - 1))
        return max(1, min(self.max_n, n))

    def _set(self, n):
        try:
            self.training_env.env_method("set_active_roster", int(n))
        except Exception:
            pass

    def _on_training_start(self) -> None:
        self._set(1)

    def _on_rollout_start(self) -> None:
        self._set(self._active())

    def _on_step(self) -> bool:
        return True


class GoalWidthCurriculumCallback(BaseCallback):
    """Narrow the scorable goal half-width from `start_hw` (wide, easy to score) to
    `end_hw` (regulation) over the first `curriculum_frac` of training. A wide goal
    lets crude/off-center pushes score early so the policy sees +30 and anchors,
    then the target shrinks back to the real net."""
    def __init__(self, total_timesteps, start_hw, end_hw, curriculum_frac=0.5):
        super().__init__()
        self.total = max(1, int(total_timesteps))
        self.start_hw = float(start_hw)
        self.end_hw = float(end_hw)
        self.frac = max(1e-6, float(curriculum_frac))

    def _hw(self):
        prog = min(1.0, (self.num_timesteps / self.total) / self.frac)
        return self.start_hw + (self.end_hw - self.start_hw) * prog

    def _set(self, hw):
        try:
            self.training_env.env_method("set_goal_half_width", float(hw))
        except Exception:
            pass

    def _on_training_start(self) -> None:
        self._set(self.start_hw)

    def _on_rollout_start(self) -> None:
        self._set(self._hw())

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
    parser.add_argument("--opponent-curriculum-frac", type=float, default=0.45,
                        help="fraction of training over which the heuristic opponent ramps "
                             "from weak (--opponent-floor) to full strength (1.0)")
    parser.add_argument("--opponent-floor", type=float, default=0.15,
                        help="starting difficulty of the heuristic opponent (0=frozen, 1=full)")
    parser.add_argument("--roster-curriculum-frac", type=float, default=0.5,
                        help="fraction of training over which the active roster grows "
                             "1v1 -> full NvN (both teams). Set >=1 to hold at full roster.")
    parser.add_argument("--roster-start-full", action="store_true",
                        help="disable the roster curriculum and train full NvN from the start")
    parser.add_argument("--goal-width-start", type=float, default=11.0,
                        help="starting scorable goal half-width in Z (wide, easy to score). "
                             "Clamped in-engine to <= half the field depth.")
    parser.add_argument("--goal-width-end", type=float, default=2.8,
                        help="final scorable goal half-width (regulation ~2.8).")
    parser.add_argument("--goal-width-curriculum-frac", type=float, default=0.5,
                        help="fraction of training over which the goal narrows start->end. "
                             "Set --goal-width-start == --goal-width-end to disable.")
    parser.add_argument("--ent-coef", type=float, default=0.005,
                        help="PPO entropy coefficient (lower = less exploration pressure; "
                             "prevents action-std runaway once the reward signal is findable)")
    parser.add_argument("--ent-coef-end", type=float, default=None,
                        help="if set, anneal ent_coef from --ent-coef down to this over "
                             "--ent-anneal-frac of training (keyed on absolute timesteps; "
                             "resumes correctly). Sharpens the policy as scales get harder.")
    parser.add_argument("--ent-anneal-frac", type=float, default=0.5,
                        help="fraction of training over which ent_coef anneals to --ent-coef-end")
    parser.add_argument("--learning-rate", type=float, default=3e-4,
                        help="PPO learning rate. Lower (e.g. 1e-4) to tame large updates "
                             "(high approx_kl / clip_fraction) once the policy is sharp.")
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
        # Eval always measures true performance: full-strength, full-roster, regulation goal.
        eval_env.set_opponent_difficulty(1.0)
        eval_env.set_active_roster(eval_env.players_per_team)
        eval_env.set_goal_half_width(args.goal_width_end)
    except Exception:
        pass

    # Callbacks
    anneal_cb = ShapingAnnealCallback(args.timesteps, args.shaping_anneal_frac)
    curriculum_cb = OpponentCurriculumCallback(
        args.timesteps, args.opponent_curriculum_frac, args.opponent_floor
    )
    roster_cb = RosterCurriculumCallback(
        args.timesteps, eval_env.players_per_team, args.roster_curriculum_frac,
    )
    goal_width_cb = GoalWidthCurriculumCallback(
        args.timesteps, args.goal_width_start, args.goal_width_end, args.goal_width_curriculum_frac,
    )
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
        curriculum_cb,
    ]
    # Player-count curriculum (1v1 -> full NvN). Omit to train full roster from start.
    if not args.roster_start_full:
        callbacks.append(roster_cb)
    # Goal-size curriculum (wide -> regulation). Skip when start == end.
    if abs(args.goal_width_start - args.goal_width_end) > 1e-6:
        callbacks.append(goal_width_cb)
    # Entropy annealing (sharpen the policy as scales harden). Enabled by --ent-coef-end.
    if args.ent_coef_end is not None:
        callbacks.append(EntropyAnnealCallback(
            args.timesteps, args.ent_coef, args.ent_coef_end, args.ent_anneal_frac))

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
        learning_rate=args.learning_rate,
        n_steps=2048,
        batch_size=256,
        n_epochs=10,
        gamma=0.99,
        gae_lambda=0.95,
        clip_range=0.2,
        ent_coef=args.ent_coef,
        policy_kwargs=dict(
            net_arch=dict(pi=[256, 256], vf=[256, 256])
        ),
        tensorboard_log=f"./runs/{run_id}",
    )

    # Resume from a checkpoint if requested (replaces the fresh model above).
    if args.resume:
        print(f"Resuming from checkpoint: {args.resume}")
        model = PPO.load(args.resume, env=env, tensorboard_log=f"./runs/{run_id}")
        # Override the entropy coefficient on resume (the checkpoint restores the old
        # one). Lets us dial exploration down once scoring is found, to stop std runaway.
        model.ent_coef = args.ent_coef
        print(f"Overriding ent_coef -> {args.ent_coef}")
        # Override the learning rate on resume (checkpoint restores the optimizer's old
        # lr). Lower it to tame large updates (high approx_kl / clip_fraction).
        from stable_baselines3.common.utils import get_schedule_fn
        model.learning_rate = args.learning_rate
        model.lr_schedule = get_schedule_fn(args.learning_rate)
        for pg in model.policy.optimizer.param_groups:
            pg["lr"] = args.learning_rate
        print(f"Overriding learning_rate -> {args.learning_rate}")

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
