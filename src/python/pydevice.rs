use crate::{
    EventWindowIterator, RawFileReader,
    buffer::PooledBuffer,
    raw::{BufferReplenisher, IterSync},
    types::{DeviceFileError, EventCD},
};
use crossbeam::channel::Receiver;
use numpy::{Element, PyArray1, PyArrayDescr};
use pyo3::{
    exceptions::{PyIOError, PyRuntimeError, PyValueError},
    prelude::*,
    types::{PyDict, PyModule},
};
use std::{
    mem::{offset_of, size_of},
    sync::Arc,
};

impl From<DeviceFileError> for PyErr {
    fn from(value: DeviceFileError) -> Self {
        match value {
            DeviceFileError::Io(error) => PyIOError::new_err(error),
            DeviceFileError::TryRecv(error) => PyIOError::new_err(error.to_string()),
            DeviceFileError::Format(error) => PyIOError::new_err(error),
            DeviceFileError::UnknownGeometry() => {
                PyValueError::new_err("Unknown or unsupported geometry")
            }
            DeviceFileError::GeometryParsing(error) => PyValueError::new_err(error.to_string()),
            DeviceFileError::EOF() => PyIOError::new_err("EOF"),
            DeviceFileError::UnsupportedFacility(error) => PyValueError::new_err(error),
            DeviceFileError::LockError => {
                PyRuntimeError::new_err("Failed to Lock Mutex. Possibly Poisoned")
            }
            DeviceFileError::FacilityTypeMismatch(error) => {
                PyValueError::new_err(error.to_string())
            }
            DeviceFileError::UnregisteredFacility(error) => PyValueError::new_err(error),
            DeviceFileError::FacilityError(error) => PyRuntimeError::new_err(error.to_string()),
            DeviceFileError::UnsupportedBehavior(error) => PyRuntimeError::new_err(error),
            DeviceFileError::NotInitialized => PyRuntimeError::new_err(
                "Device is not initialized. Cannot call methods on unitialize device.",
            ),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
#[pyclass(from_py_object)]
pub struct PyEventCD {
    x: u16,
    y: u16,
    p: u8,
    t: usize,
}

impl From<EventCD> for PyEventCD {
    fn from(value: EventCD) -> Self {
        Self {
            x: value.x as u16, // very unlikely to overflow, 8k is << 2^16
            y: value.y as u16,
            p: value.p.into(),
            t: value.t,
        }
    }
}

impl From<&EventCD> for PyEventCD {
    fn from(value: &EventCD) -> Self {
        Self {
            x: value.x as u16, // very unlikely to overflow, 8k is << 2^16
            y: value.y as u16,
            p: value.p.into(),
            t: value.t,
        }
    }
}

// Safety: A #[repr(C)] struct containing only valid NumPy element types
// satisfies the memory alignment and layout rules required by NumPy.
unsafe impl Element for PyEventCD {
    const IS_COPY: bool = true;
    fn vec_from_slice(py: Python<'_>, slc: &[Self]) -> Vec<Self> {
        slc.iter().map(|elem| elem.clone_ref(py)).collect()
    }

    fn array_from_view<D>(
        py: Python<'_>,
        view: numpy::ndarray::ArrayView<'_, Self, D>,
    ) -> numpy::ndarray::Array<Self, D>
    where
        D: numpy::ndarray::Dimension,
    {
        view.map(|elem| elem.clone_ref(py))
    }

    fn get_dtype(py: Python<'_>) -> Bound<'_, numpy::PyArrayDescr> {
        let np = py.import("numpy").expect("Failed to import numpy");
        let dtype_spec = PyDict::new(py);
        dtype_spec.set_item("names", ("x", "y", "p", "t")).unwrap();
        dtype_spec
            .set_item("formats", ("u2", "u2", "u1", "u8"))
            .unwrap();
        dtype_spec
            .set_item(
                "offsets",
                (
                    offset_of!(PyEventCD, x),
                    offset_of!(PyEventCD, y),
                    offset_of!(PyEventCD, p),
                    offset_of!(PyEventCD, t),
                ),
            )
            .unwrap();
        dtype_spec
            .set_item("itemsize", size_of::<PyEventCD>())
            .unwrap();

        np.call_method1("dtype", (dtype_spec,))
            .unwrap()
            .cast_into::<PyArrayDescr>()
            .unwrap()
    }

    fn clone_ref(&self, _py: Python<'_>) -> Self {
        *self
    }
}

#[pyclass(name = "CDEventReceiver")]
pub struct PyCDEventReceiver {
    inner: Receiver<Arc<PooledBuffer<EventCD>>>,
}

#[pymethods]
impl PyCDEventReceiver {
    /// Receive one already-decoded batch without waiting.
    fn try_recv<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        match self.inner.try_recv() {
            Ok(buffer) if !buffer.is_empty() => {
                Ok(Some(events_to_numpy(py, buffer.iter().cloned().collect())))
            }
            Ok(_) => Ok(None),
            Err(crossbeam::channel::TryRecvError::Empty) => Ok(None),
            Err(crossbeam::channel::TryRecvError::Disconnected) => Ok(None),
        }
    }

    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.try_recv(py)?
            .ok_or_else(|| pyo3::exceptions::PyStopIteration::new_err(()))
    }

    fn __aiter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __anext__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let buffer = self
            .inner
            .recv()
            .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
        async_result(
            py,
            if buffer.is_empty() {
                None
            } else {
                Some(events_to_numpy(py, buffer.iter().cloned().collect()))
            },
        )
    }
}

fn events_to_numpy<'py>(py: Python<'py>, events: Vec<EventCD>) -> Bound<'py, PyAny> {
    PyArray1::from_vec(py, events.iter().map(PyEventCD::from).collect::<Vec<_>>()).into_any()
}

fn async_result<'py>(
    py: Python<'py>,
    value: Option<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyAny>> {
    if let Some(value) = value {
        py.import("asyncio")?.getattr("sleep")?.call1((0, value))
    } else {
        Err(pyo3::exceptions::PyStopAsyncIteration::new_err(()))
    }
}

fn next_events<const BUFFER_SIZE: usize, State>(
    iterator: &mut EventWindowIterator<BUFFER_SIZE, State>,
) -> PyResult<Vec<EventCD>>
where
    EventWindowIterator<BUFFER_SIZE, State>: BufferReplenisher,
{
    iterator.next_batch().map_err(PyErr::from)
}

#[pyclass(name = "CDEventIterator")]
pub struct PyCDEventIterator {
    inner: EventWindowIterator<131_072, IterSync>,
}

#[pymethods]
impl PyCDEventIterator {
    /// Return this iterator, allowing `for batch in reader.iter()`.
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let events = next_events(&mut self.inner)?;
        if events.is_empty() {
            return Err(pyo3::exceptions::PyStopIteration::new_err(()));
        }
        Ok(events_to_numpy(py, events))
    }

    fn next_batch<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let events = next_events(&mut self.inner)?;
        Ok(events_to_numpy(py, events))
    }

    fn shape(&self) -> (u32, u32) {
        self.inner.shape()
    }
}

#[pyclass(name = "AsyncCDEventIterator")]
pub struct PyAsyncCDEventIterator {
    inner: Receiver<Arc<PooledBuffer<EventCD>>>,
    shape: (u32, u32),
}

#[pymethods]
impl PyAsyncCDEventIterator {
    fn __aiter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __anext__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let buffer = self
            .inner
            .recv()
            .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
        async_result(
            py,
            if buffer.is_empty() {
                None
            } else {
                Some(events_to_numpy(py, buffer.iter().cloned().collect()))
            },
        )
    }

    fn next_batch<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        self.__anext__(py)
    }

    fn shape(&self) -> PyResult<(u32, u32)> {
        Ok(self.shape)
    }
}

#[pyclass(name = "RawFileReader")]
pub struct PyRawFileReader {
    inner: RawFileReader<131_072>,
}

#[pymethods]
impl PyRawFileReader {
    #[new]
    #[pyo3(signature = (path = None, index = false))]
    fn new(path: Option<&str>, index: bool) -> PyResult<Self> {
        let inner = match path {
            Some(path) => RawFileReader::<131_072>::try_from_file(path, index)?,
            None => RawFileReader::<131_072>::new(),
        };

        Ok(PyRawFileReader { inner })
    }

    fn try_open(&mut self, file_path: &str, index: bool) -> PyResult<()> {
        self.inner.try_open(file_path, index)?;
        Ok(())
    }

    fn ready(&self) -> bool {
        self.inner.ready()
    }

    fn t_min(&self) -> Option<usize> {
        self.inner.t_min()
    }

    fn t_max(&self) -> Option<usize> {
        self.inner.t_max()
    }

    fn seek(&mut self, ts: u32) -> PyResult<()> {
        self.inner.seek(ts)?;
        Ok(())
    }

    fn seek_to_next_ext(&mut self) -> PyResult<()> {
        self.inner.seek_to_next_ext()?;
        Ok(())
    }

    fn load_batch(&mut self) -> PyResult<()> {
        self.inner.load_batch()?;
        Ok(())
    }

    fn cd_receiver(&mut self) -> PyResult<PyCDEventReceiver> {
        Ok(PyCDEventReceiver {
            inner: self.inner.cd_receiver()?,
        })
    }

    /// Create a synchronous batch iterator.
    fn iter(&mut self) -> PyResult<PyCDEventIterator> {
        Ok(PyCDEventIterator {
            inner: self.inner.as_windows()?.into_sync(),
        })
    }

    /// Create an asynchronous batch iterator.
    fn aiter(&mut self) -> PyResult<PyAsyncCDEventIterator> {
        let shape = self.inner.shape();
        Ok(PyAsyncCDEventIterator {
            inner: self.inner.cd_receiver()?,
            shape,
        })
    }
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyEventCD>()?;
    module.add_class::<PyCDEventReceiver>()?;
    module.add_class::<PyCDEventIterator>()?;
    module.add_class::<PyAsyncCDEventIterator>()?;
    module.add_class::<PyRawFileReader>()?;
    Ok(())
}
