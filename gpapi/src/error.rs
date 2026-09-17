use std::error::Error as StdError;
use std::fmt;
use std::io::Error as IOError;

#[derive(Debug)]
pub enum ErrorKind {
    InvalidApp,
    Authentication,
    TermsOfService,
    InvalidResponse,
    LoginRequired,
    IO(IOError),
    Str(String),
    Other(Box<dyn StdError + Send + Sync>),
}

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
}

impl Error {
    #[must_use]
    pub const fn new(k: ErrorKind) -> Self {
        Self { kind: k }
    }

    #[must_use]
    pub const fn kind(&self) -> &ErrorKind {
        &self.kind
    }
}

impl From<IOError> for Error {
    fn from(err: IOError) -> Self {
        Self {
            kind: ErrorKind::IO(err),
        }
    }
}

impl From<Box<dyn StdError + Send + Sync>> for Error {
    fn from(err: Box<dyn StdError + Send + Sync>) -> Self {
        Self {
            kind: ErrorKind::Other(err),
        }
    }
}

impl From<&str> for Error {
    fn from(err: &str) -> Self {
        Self {
            kind: ErrorKind::Str(err.to_string()),
        }
    }
}

impl From<String> for Error {
    fn from(err: String) -> Self {
        Self {
            kind: ErrorKind::Str(err),
        }
    }
}

impl StdError for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.kind() {
            ErrorKind::InvalidApp => write!(f, "Invalid app response"),
            ErrorKind::Authentication => write!(
                f,
                "Could not authenticate with Google. Please provide a new oAuth token."
            ),
            ErrorKind::TermsOfService => write!(
                f,
                "Must accept Google Play Terms of Service before proceeding."
            ),
            ErrorKind::InvalidResponse => write!(f, "Invalid response from the remote host"),
            ErrorKind::LoginRequired => write!(f, "Logging in is required for this action"),
            ErrorKind::IO(err) => err.fmt(f),
            ErrorKind::Str(err) => err.fmt(f),
            ErrorKind::Other(err) => err.fmt(f),
        }
    }
}
