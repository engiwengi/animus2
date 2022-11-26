#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Generic {0}")]
    Generic(String),

    #[error(transparent)]
    Speedy(#[from] speedy::Error),

    #[error(transparent)]
    IO(#[from] std::io::Error),
}

pub(crate) type Result<T> = std::result::Result<T, Error>;
