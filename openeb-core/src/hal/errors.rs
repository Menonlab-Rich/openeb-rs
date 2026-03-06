use thiserror::Error;

#[derive(Debug)]
pub enum HALErrorCode {
    CameraError = 0x100000,
    FailedInitialization = 0x101000,
    CameraNotFound = 0x101001,
    GoldenFallbackBooted = 0x101002,
    InternalInitializationError = 0x101100,
    InvalidArgument = 0x102000,
    ValueOutOfRange = 0x102001,
    NonExistingValue = 0x102002,
    OperationNotPermitted = 0x102003,
    UnsupportedValue = 0x102004,
    DeprecatedFunctionCalled = 0x103000,
    OperationNotImplemented = 0x104000,
    MaximumRetriesExceeded = 0x105000,
}

#[derive(Debug, Error, Clone, Copy, PartialEq)]
pub enum DecoderProtocolViolation {
    #[error("Null protocol violation")]
    NullProtocolViolation = 0,
    #[error("Non-monotonic time high violation")]
    NonMonotonicTimeHigh = 1,
    #[error("Partial vector 12_12_8 violation")]
    PartialVect = 2,
    #[error("Partial continued 12_12_4 violation")]
    PartialContinued = 3,
    #[error("Non continuous time high violation")]
    NonContinuousTimeHigh = 4,
    #[error("Missing Y address violation")]
    MissingYAddr = 5,
    #[error("Invalid vector base violation")]
    InvalidVectBase = 6,
    #[error("Out of bounds event coordinate violation")]
    OutOfBoundsEventCoordinate = 7,
}

#[derive(Error, Debug)]
pub enum HalError {
    // 1. Map specific OpenEB error codes here
    #[error("Device not found")]
    DeviceNotFound,

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Feature not supported")]
    UnsupportedFeature,

    #[error("Decoder protocol violation: {0}")]
    ProtocolViolation(#[from] DecoderProtocolViolation),

    // 2. Automatic conversion for standard errors (e.g., IO)
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),

    // 3. The "Any Error" Catch-All
    // The #[from] attribute here magicially allows users to use '?'
    // on ANY error type that implements std::error::Error, and it will
    // auto-box it into this variant.
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

impl From<&str> for HalError {
    fn from(value: &str) -> Self {
        HalError::Other(value.into())
    }
}

impl From<String> for HalError {
    fn from(value: String) -> Self {
        HalError::Other(value.into())
    }
}

// Define a shorthand alias for convenience
pub type HalResult<T> = Result<T, HalError>;
