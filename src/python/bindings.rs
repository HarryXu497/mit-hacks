#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use numpy::{PyArray1, PyReadonlyArray1, IntoPyArray};

#[cfg(feature = "python")]
use crate::rl::{CubeSoccerEnv, EnvConfig};
#[cfg(feature = "python")]
use crate::game::config as cube_soccer_shapes;
#[cfg(feature = "python")]
use pyo3::exceptions::PyValueError;
#[cfg(feature = "python")]
use crate::game::Team;
#[cfg(feature = "python")]
use crate::systems::heuristic_ai::{Tactic, TacticParams};

#[cfg(feature = "python")]
#[pyclass(unsendable)]
pub struct PyCubeSoccerEnv {
    env: CubeSoccerEnv,
}

#[cfg(feature = "python")]
fn parse_team(team: &str) -> PyResult<Team> {
    match team.trim().to_lowercase().as_str() {
        "orange" => Ok(Team::Orange),
        "blue" => Ok(Team::Blue),
        other => Err(PyValueError::new_err(format!("unknown team: {other:?} (expected 'orange' or 'blue')"))),
    }
}

#[cfg(feature = "python")]
fn parse_preset(name: &str) -> PyResult<Tactic> {
    Tactic::from_name(name)
        .ok_or_else(|| PyValueError::new_err(format!("unknown tactic: {name:?}")))
}

#[cfg(feature = "python")]
#[pymethods]
impl PyCubeSoccerEnv {
    #[new]
    #[pyo3(signature = (headless=true, render_mode=None))]
    fn new(headless: bool, render_mode: Option<String>) -> PyResult<Self> {
        let config = EnvConfig {
            headless,
            render_mode,
            ..Default::default()
        };
        Ok(Self {
            env: CubeSoccerEnv::new(config),
        })
    }

    fn reset<'py>(
        &mut self,
        py: Python<'py>,
        seed: Option<u64>,
    ) -> PyResult<&'py PyArray1<f32>> {
        let obs = self.env.reset(seed); // Vec<f32>, len NUM_AGENTS * OBSERVATION_SIZE
        Ok(obs.into_pyarray(py))
    }

    fn step<'py>(
        &mut self,
        py: Python<'py>,
        actions: PyReadonlyArray1<f32>,
    ) -> PyResult<(
        &'py PyArray1<f32>, // flat observations (NUM_AGENTS * OBSERVATION_SIZE)
        Vec<f32>,           // per-agent rewards (NUM_AGENTS)
        bool,               // done
        bool,               // truncated
        PyObject,           // info dict
    )> {
        let actions_slice: &[f32] = actions.as_slice()?;
        let result = self.env.step(actions_slice);

        let info = pyo3::types::PyDict::new(py);
        info.set_item("score_orange", result.info.score[0])?;
        info.set_item("score_blue", result.info.score[1])?;
        info.set_item("time_remaining", result.info.time_remaining)?;
        if let Some(winner) = result.info.winner {
            info.set_item("winner", format!("{:?}", winner))?;
        }

        Ok((
            result.observations.into_pyarray(py),
            result.rewards,
            result.done,
            result.truncated,
            info.into(),
        ))
    }

    fn render(&mut self) -> PyResult<()> {
        self.env.render();
        Ok(())
    }

    #[getter]
    fn observation_space(&self) -> PyResult<(Vec<f32>, Vec<f32>, Vec<usize>)> {
        Ok(self.env.get_observation_space())
    }

    #[getter]
    fn action_space(&self) -> PyResult<(Vec<f32>, Vec<f32>, Vec<usize>)> {
        Ok(self.env.get_action_space())
    }

    #[getter]
    fn num_agents(&self) -> usize { cube_soccer_shapes::NUM_AGENTS }
    #[getter]
    fn players_per_team(&self) -> usize { cube_soccer_shapes::PLAYERS_PER_TEAM }
    #[getter]
    fn observation_size(&self) -> usize { cube_soccer_shapes::OBSERVATION_SIZE }
    #[getter]
    fn action_size(&self) -> usize { cube_soccer_shapes::ACTION_SIZE }

    /// Set a team's whole-team tactic from a preset name (e.g. "High Press").
    fn set_team_preset(&mut self, team: &str, name: &str) -> PyResult<()> {
        self.env.set_team_preset(parse_team(team)?, parse_preset(name)?);
        Ok(())
    }

    /// Set a team's whole-team tactic params directly.
    fn set_team_params(&mut self, team: &str, defender_depth: f32, attacker_push: f32, width: f32, spacing: f32, press: f32, line_height: f32, commitment: f32) -> PyResult<()> {
        self.env.set_team_params(parse_team(team)?, TacticParams { defender_depth, attacker_push, width, spacing, press, line_height, commitment });
        Ok(())
    }

    /// Set a team's base tactic to a weighted blend of presets.
    /// `presets` and `weights` must be equal length (e.g. ["Low Block","Wide"], [0.7,0.3]).
    fn set_team_blend(&mut self, team: &str, presets: Vec<String>, weights: Vec<f32>) -> PyResult<()> {
        if presets.len() != weights.len() {
            return Err(PyValueError::new_err("presets and weights must be the same length"));
        }
        let mut parts: Vec<(TacticParams, f32)> = Vec::with_capacity(presets.len());
        for (name, w) in presets.iter().zip(weights) {
            parts.push((parse_preset(name)?.params(), w));
        }
        self.env.set_team_blend(parse_team(team)?, &parts);
        Ok(())
    }

    /// Override a single player's (by index) tactic params.
    fn set_player_params(&mut self, team: &str, index: usize, defender_depth: f32, attacker_push: f32, width: f32, spacing: f32, press: f32, line_height: f32, commitment: f32) -> PyResult<()> {
        self.env.set_player_params(parse_team(team)?, index, TacticParams { defender_depth, attacker_push, width, spacing, press, line_height, commitment });
        Ok(())
    }

    /// Remove all per-player overrides for a team.
    fn clear_player_overrides(&mut self, team: &str) -> PyResult<()> {
        self.env.clear_player_overrides(parse_team(team)?);
        Ok(())
    }

    /// Set the dense-shaping weight (1.0 = full, 0.0 = pure goal objective).
    fn set_shaping_weight(&mut self, weight: f32) -> PyResult<()> {
        self.env.set_shaping_weight(weight);
        Ok(())
    }

    /// Set the heuristic opponent's difficulty (1.0 = full strength, 0.0 = frozen).
    /// Used by the training curriculum to weaken Blue early, then ramp to full.
    fn set_opponent_difficulty(&mut self, difficulty: f32) -> PyResult<()> {
        self.env.set_opponent_difficulty(difficulty);
        Ok(())
    }
}

#[cfg(feature = "python")]
#[pymodule]
fn cube_soccer(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyCubeSoccerEnv>()?;
    Ok(())
}
