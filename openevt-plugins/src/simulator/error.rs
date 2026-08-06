use thiserror::Error;

#[derive(Error, Debug)]
pub enum SimError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("FFmpeg error: {0}")]
    Ffmpeg(#[from] ffmpeg_next::Error),
    #[error("invalid simulator configuration: {0}")]
    InvalidConfiguration(String),
}
