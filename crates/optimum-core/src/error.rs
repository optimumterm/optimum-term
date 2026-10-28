use std::{fmt, io};

#[derive(Debug)]
pub enum Error {
    Vt(ghostty_vt::Error),
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Vt(e) => write!(f, "terminal: {e}"),
            Error::Io(e) => write!(f, "pty: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Vt(e) => Some(e),
            Error::Io(e) => Some(e),
        }
    }
}

impl From<ghostty_vt::Error> for Error {
    fn from(e: ghostty_vt::Error) -> Self {
        Error::Vt(e)
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}
