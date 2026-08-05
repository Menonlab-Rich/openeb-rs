use thiserror::Error;

#[derive(Error, Debug)]
pub enum SimError {
    Io(#[from] std::io::Error),
}
