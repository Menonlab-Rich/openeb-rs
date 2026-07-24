use crate::ffi;
use cxx::UniquePtr;

struct Camera {
    inner: UniquePtr<ffi::Metavision::Camera>,
}

impl Camera {
    pub fn from_first_available() -> Result<Self, cxx::Exception> {
        todo!();
    }
}
