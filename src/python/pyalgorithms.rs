use crate::{
    algorithms::{
        EvtProcessor, EvtTransformer, filters,
        time_surface::{
            BaseTimeSurface, BaseTimeSurfaceTransformer, ExponentialDecayTimeSurface,
            ExponentialDecayTimeSurfaceTransformer, LinearDecayTimeSurface,
            LinearDecayTimeSurfaceTransformer,
        },
    },
    hal::types::{EventCoordinate, EventTimestamp},
    python::pydevice::{PyEventCD, events_from_numpy, events_to_numpy},
};
use numpy::PyReadonlyArray1;
use pyo3::{prelude::*, types::PyModule};

fn process_filter<'py, F>(
    py: Python<'py>,
    filter: &mut F,
    events: PyReadonlyArray1<'py, PyEventCD>,
) -> PyResult<Bound<'py, PyAny>>
where
    F: EvtProcessor,
{
    let events = events_from_numpy(events)?;
    let output = filter
        .process_events(Box::new(events.into_iter()))
        .collect();
    Ok(events_to_numpy(py, output))
}

#[pyclass(name = "PolarityFilter")]
pub struct PyPolarityFilter {
    inner: filters::PolarityFilter,
}

#[pymethods]
impl PyPolarityFilter {
    #[new]
    fn new(polarity: bool) -> Self {
        Self {
            inner: filters::PolarityFilter::new(polarity),
        }
    }

    fn process<'py>(
        &mut self,
        py: Python<'py>,
        events: PyReadonlyArray1<'py, PyEventCD>,
    ) -> PyResult<Bound<'py, PyAny>> {
        process_filter(py, &mut self.inner, events)
    }
}

#[pyclass(name = "RoiFilter")]
pub struct PyRoiFilter {
    inner: filters::RoiFilter,
}

#[pymethods]
impl PyRoiFilter {
    #[new]
    fn new(
        x0: EventCoordinate,
        x1: EventCoordinate,
        y0: EventCoordinate,
        y1: EventCoordinate,
    ) -> Self {
        Self {
            inner: filters::RoiFilter::new(x0, x1, y0, y1),
        }
    }

    fn process<'py>(
        &mut self,
        py: Python<'py>,
        events: PyReadonlyArray1<'py, PyEventCD>,
    ) -> PyResult<Bound<'py, PyAny>> {
        process_filter(py, &mut self.inner, events)
    }
}

#[pyclass(name = "ActivityNoiseFilter")]
pub struct PyActivityNoiseFilter {
    inner: filters::ActivityNoiseFilter,
}

#[pymethods]
impl PyActivityNoiseFilter {
    #[new]
    fn new(width: EventCoordinate, height: EventCoordinate, dt: EventTimestamp) -> Self {
        Self {
            inner: filters::ActivityNoiseFilter::new(width, height, dt),
        }
    }

    fn process<'py>(
        &mut self,
        py: Python<'py>,
        events: PyReadonlyArray1<'py, PyEventCD>,
    ) -> PyResult<Bound<'py, PyAny>> {
        process_filter(py, &mut self.inner, events)
    }
}

#[pyclass(name = "SpatioTemporalContrastFilter")]
pub struct PySpatioTemporalContrastFilter {
    inner: filters::SpatioTemporalContrastFilter,
}

#[pymethods]
impl PySpatioTemporalContrastFilter {
    #[new]
    fn new(
        width: EventCoordinate,
        height: EventCoordinate,
        dt: EventTimestamp,
        cut_trail: bool,
    ) -> Self {
        Self {
            inner: filters::SpatioTemporalContrastFilter::new(width, height, dt, cut_trail),
        }
    }

    fn process<'py>(
        &mut self,
        py: Python<'py>,
        events: PyReadonlyArray1<'py, PyEventCD>,
    ) -> PyResult<Bound<'py, PyAny>> {
        process_filter(py, &mut self.inner, events)
    }
}

#[pyclass(name = "TrailFilter")]
pub struct PyTrailFilter {
    inner: filters::TrailFilter,
}

#[pymethods]
impl PyTrailFilter {
    #[new]
    fn new(width: EventCoordinate, height: EventCoordinate, dt: EventTimestamp) -> Self {
        Self {
            inner: filters::TrailFilter::new(width, height, dt),
        }
    }

    fn process<'py>(
        &mut self,
        py: Python<'py>,
        events: PyReadonlyArray1<'py, PyEventCD>,
    ) -> PyResult<Bound<'py, PyAny>> {
        process_filter(py, &mut self.inner, events)
    }
}

#[pyclass(name = "AntiFlickerFilter")]
pub struct PyAntiFlickerFilter {
    inner: filters::AntiFlickerFilter,
}

#[pymethods]
impl PyAntiFlickerFilter {
    #[new]
    fn new(
        width: EventCoordinate,
        height: EventCoordinate,
        min_measurements: u32,
        min_freq: u32,
        max_freq: u32,
    ) -> Self {
        Self {
            inner: filters::AntiFlickerFilter::new(
                width,
                height,
                min_measurements,
                min_freq,
                max_freq,
            ),
        }
    }

    fn process<'py>(
        &mut self,
        py: Python<'py>,
        events: PyReadonlyArray1<'py, PyEventCD>,
    ) -> PyResult<Bound<'py, PyAny>> {
        process_filter(py, &mut self.inner, events)
    }
}

#[pyclass(name = "BaseTimeSurface")]
pub struct PyBaseTimeSurface {
    inner: BaseTimeSurface,
}

impl From<BaseTimeSurface> for PyBaseTimeSurface {
    fn from(inner: BaseTimeSurface) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyBaseTimeSurface {
    #[new]
    fn new(width: EventCoordinate, height: EventCoordinate) -> Self {
        Self {
            inner: BaseTimeSurface::new(width, height),
        }
    }

    fn update(&mut self, x: EventCoordinate, y: EventCoordinate, p: bool, ts: EventTimestamp) {
        self.inner.update(x, y, p, ts);
    }

    fn get(&self, x: EventCoordinate, y: EventCoordinate, p: bool) -> EventTimestamp {
        self.inner.get(x, y, p)
    }

    fn width(&self) -> EventCoordinate {
        self.inner.width()
    }

    fn height(&self) -> EventCoordinate {
        self.inner.height()
    }
}

#[pyclass(name = "LinearDecayTimeSurface")]
pub struct PyLinearDecayTimeSurface {
    inner: LinearDecayTimeSurface,
}

impl From<LinearDecayTimeSurface> for PyLinearDecayTimeSurface {
    fn from(inner: LinearDecayTimeSurface) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyLinearDecayTimeSurface {
    #[new]
    fn new(width: EventCoordinate, height: EventCoordinate, decay_time: EventTimestamp) -> Self {
        Self {
            inner: LinearDecayTimeSurface::new(width, height, decay_time),
        }
    }

    fn update(&mut self, x: EventCoordinate, y: EventCoordinate, p: bool, ts: EventTimestamp) {
        self.inner.update(x, y, p, ts);
    }

    fn decay_to(&mut self, ts: EventTimestamp) {
        self.inner.decay_to(ts);
    }

    fn get(&self, x: EventCoordinate, y: EventCoordinate, p: bool) -> f64 {
        self.inner.get(x, y, p)
    }

    fn timestamp(&self, x: EventCoordinate, y: EventCoordinate, p: bool) -> EventTimestamp {
        self.inner.timestamp(x, y, p)
    }

    fn width(&self) -> EventCoordinate {
        self.inner.width()
    }

    fn height(&self) -> EventCoordinate {
        self.inner.height()
    }
}

#[pyclass(name = "ExponentialDecayTimeSurface")]
pub struct PyExponentialDecayTimeSurface {
    inner: ExponentialDecayTimeSurface,
}

impl From<ExponentialDecayTimeSurface> for PyExponentialDecayTimeSurface {
    fn from(inner: ExponentialDecayTimeSurface) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyExponentialDecayTimeSurface {
    #[new]
    fn new(width: EventCoordinate, height: EventCoordinate, tau: EventTimestamp) -> Self {
        Self {
            inner: ExponentialDecayTimeSurface::new(width, height, tau),
        }
    }

    fn update(&mut self, x: EventCoordinate, y: EventCoordinate, p: bool, ts: EventTimestamp) {
        self.inner.update(x, y, p, ts);
    }

    fn decay_to(&mut self, ts: EventTimestamp) {
        self.inner.decay_to(ts);
    }

    fn get(&self, x: EventCoordinate, y: EventCoordinate, p: bool) -> f64 {
        self.inner.get(x, y, p)
    }

    fn timestamp(&self, x: EventCoordinate, y: EventCoordinate, p: bool) -> EventTimestamp {
        self.inner.timestamp(x, y, p)
    }

    fn width(&self) -> EventCoordinate {
        self.inner.width()
    }

    fn height(&self) -> EventCoordinate {
        self.inner.height()
    }
}

#[pyclass(name = "BaseTimeSurfaceTransformer")]
pub struct PyBaseTimeSurfaceTransformer {
    inner: BaseTimeSurfaceTransformer,
}

#[pymethods]
impl PyBaseTimeSurfaceTransformer {
    #[new]
    fn new(width: EventCoordinate, height: EventCoordinate) -> Self {
        Self {
            inner: BaseTimeSurfaceTransformer::new(width, height),
        }
    }

    fn transform(&self, events: PyReadonlyArray1<'_, PyEventCD>) -> PyResult<PyBaseTimeSurface> {
        let events = events_from_numpy(events)?;
        Ok(self
            .inner
            .transform_events(Box::new(events.into_iter()))
            .into())
    }
}

#[pyclass(name = "LinearDecayTimeSurfaceTransformer")]
pub struct PyLinearDecayTimeSurfaceTransformer {
    inner: LinearDecayTimeSurfaceTransformer,
}

#[pymethods]
impl PyLinearDecayTimeSurfaceTransformer {
    #[new]
    fn new(width: EventCoordinate, height: EventCoordinate, decay_time: EventTimestamp) -> Self {
        Self {
            inner: LinearDecayTimeSurfaceTransformer::new(width, height, decay_time),
        }
    }

    fn transform(
        &self,
        events: PyReadonlyArray1<'_, PyEventCD>,
    ) -> PyResult<PyLinearDecayTimeSurface> {
        let events = events_from_numpy(events)?;
        Ok(self
            .inner
            .transform_events(Box::new(events.into_iter()))
            .into())
    }
}

#[pyclass(name = "ExponentialDecayTimeSurfaceTransformer")]
pub struct PyExponentialDecayTimeSurfaceTransformer {
    inner: ExponentialDecayTimeSurfaceTransformer,
}

#[pymethods]
impl PyExponentialDecayTimeSurfaceTransformer {
    #[new]
    fn new(width: EventCoordinate, height: EventCoordinate, tau: EventTimestamp) -> Self {
        Self {
            inner: ExponentialDecayTimeSurfaceTransformer::new(width, height, tau),
        }
    }

    fn transform(
        &self,
        events: PyReadonlyArray1<'_, PyEventCD>,
    ) -> PyResult<PyExponentialDecayTimeSurface> {
        let events = events_from_numpy(events)?;
        Ok(self
            .inner
            .transform_events(Box::new(events.into_iter()))
            .into())
    }
}

fn add_filter_classes(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyPolarityFilter>()?;
    module.add_class::<PyRoiFilter>()?;
    module.add_class::<PyActivityNoiseFilter>()?;
    module.add_class::<PySpatioTemporalContrastFilter>()?;
    module.add_class::<PyTrailFilter>()?;
    module.add_class::<PyAntiFlickerFilter>()?;
    Ok(())
}

fn add_surface_classes(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyBaseTimeSurface>()?;
    module.add_class::<PyLinearDecayTimeSurface>()?;
    module.add_class::<PyExponentialDecayTimeSurface>()?;
    Ok(())
}

fn add_transformer_classes(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyBaseTimeSurfaceTransformer>()?;
    module.add_class::<PyLinearDecayTimeSurfaceTransformer>()?;
    module.add_class::<PyExponentialDecayTimeSurfaceTransformer>()?;
    Ok(())
}

fn add_algorithm_classes(module: &Bound<'_, PyModule>) -> PyResult<()> {
    add_filter_classes(module)?;
    add_surface_classes(module)?;
    add_transformer_classes(module)?;
    Ok(())
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    add_algorithm_classes(module)?;

    let py = module.py();
    let algorithms = PyModule::new(py, "openevt.algorithms")?;
    add_algorithm_classes(&algorithms)?;

    let algorithms_filters = PyModule::new(py, "openevt.algorithms.filters")?;
    add_filter_classes(&algorithms_filters)?;
    algorithms.add("filters", &algorithms_filters)?;

    let filters = PyModule::new(py, "openevt.filters")?;
    add_filter_classes(&filters)?;

    module.add("algorithms", &algorithms)?;
    module.add("filters", &filters)?;

    let sys_modules = py.import("sys")?.getattr("modules")?;
    sys_modules.set_item("openevt.algorithms", algorithms)?;
    sys_modules.set_item("openevt.algorithms.filters", algorithms_filters)?;
    sys_modules.set_item("openevt.filters", filters)?;

    Ok(())
}
