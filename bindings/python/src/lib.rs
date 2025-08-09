use tellers_timeline_core::{validate_timeline, Timeline};
use pyo3::prelude::*;

#[pyclass(name = "Timeline")]
#[derive(Clone)]
struct PyTimeline {
    inner: Timeline,
}

#[pymethods]
impl PyTimeline {
    #[new]
    fn new() -> Self { Self { inner: Timeline::default() } }

    #[staticmethod]
    fn parse_json(s: &str) -> PyResult<Self> {
        let tl: Timeline = serde_json::from_str(s).map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))?;
        Ok(Self { inner: tl })
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string_pretty(&self.inner).map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e.to_string()))
    }

    fn validate(&self) -> Vec<String> {
        validate_timeline(&self.inner).into_iter().map(|e| e.to_string()).collect()
    }

    fn sanitize(&mut self) { self.inner.sanitize(); }
}

#[pymodule]
fn tellers_timeline(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyTimeline>()?;
    Ok(())
}
