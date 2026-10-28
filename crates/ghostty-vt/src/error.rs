use std::fmt;

use ghostty_vt_sys as sys;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    OutOfMemory,
    InvalidValue,
    OutOfSpace,
    NoValue,
    Io,
    LimitExceeded,
    Rejected,
    Unknown(i32),
}

impl Error {
    pub(crate) fn check(result: sys::GhosttyResult) -> Result<(), Error> {
        match result {
            sys::GHOSTTY_SUCCESS => Ok(()),
            sys::GHOSTTY_OUT_OF_MEMORY => Err(Error::OutOfMemory),
            sys::GHOSTTY_INVALID_VALUE => Err(Error::InvalidValue),
            sys::GHOSTTY_OUT_OF_SPACE => Err(Error::OutOfSpace),
            sys::GHOSTTY_NO_VALUE => Err(Error::NoValue),
            sys::GHOSTTY_IO_ERROR => Err(Error::Io),
            sys::GHOSTTY_LIMIT_EXCEEDED => Err(Error::LimitExceeded),
            sys::GHOSTTY_REJECTED => Err(Error::Rejected),
            other => Err(Error::Unknown(other)),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::OutOfMemory => f.write_str("out of memory"),
            Error::InvalidValue => f.write_str("invalid value"),
            Error::OutOfSpace => f.write_str("buffer too small"),
            Error::NoValue => f.write_str("no value"),
            Error::Io => f.write_str("I/O error"),
            Error::LimitExceeded => f.write_str("limit exceeded"),
            Error::Rejected => f.write_str("rejected by safety check"),
            Error::Unknown(code) => write!(f, "unknown libghostty-vt result {code}"),
        }
    }
}

impl std::error::Error for Error {}
