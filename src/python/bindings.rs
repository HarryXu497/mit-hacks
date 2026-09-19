#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use numpy::{PyArray1, PyReadonlyArray1, IntoPyArray};

#[cfg(feature = "python")]
use crate::rl::{CubeSoccerEnv, EnvConfig};

#[cfg(feature = "python")]
#[pyclass]
pub struct PyCubeSoccerEnv {
    env: CubeSoccerEnv,
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
        let obs = self.env.reset(seed);
        Ok(obs.to_vec().into_pyarray(py))
    }

    fn step<'py>(
        &mut self,
        py: Python<'py>,
        actions: PyReadonlyArray1<f32>,
    ) -> PyResult<(
        &'py PyArray1<f32>,  // observations
        (f32, f32),          // rewards (orange, blue)
        bool,                // done
        bool,                // truncated
        PyObject,            // info dict
    )> {
        let actions_slice: &[f32] = actions.as_slice()?;
        let mut actions_array = [0.0f32; 8];
        for (i, &v) in actions_slice.iter().take(8).enumerate() {
            actions_array[i] = v;
        }

        let result = self.env.step(&actions_array);

        let info = pyo3::types::PyDict::new(py);
        info.set_item("score_orange", result.info.score[0])?;
        info.set_item("score_blue", result.info.score[1])?;
        info.set_item("time_remaining", result.info.time_remaining)?;
        if let Some(winner) = result.info.winner {
            info.set_item("winner", format!("{:?}", winner))?;
        }

        Ok((
            result.observations.to_vec().into_pyarray(py),
            (result.rewards[0], result.rewards[1]),
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
}

#[cfg(feature = "python")]
#[pymodule]
fn cube_soccer(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyCubeSoccerEnv>()?;
    Ok(())
}
