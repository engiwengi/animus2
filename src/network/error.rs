#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Generic {0}")]
    Generic(String),

    #[error(transparent)]
    Speedy(#[from] speedy::Error),

    #[error(transparent)]
    IO(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct W<T>(pub T);
