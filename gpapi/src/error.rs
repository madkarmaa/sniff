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
    /// Google answered with a non-success HTTP status. `body` is a short,
    /// lossily decoded excerpt of the response for diagnostics.
    Http {
        status: u16,
        body: String,
    },
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

    /// The HTTP status code, if this error came from a non-success response.
    #[must_use]
    pub const fn http_status(&self) -> Option<u16> {
        match self.kind {
            ErrorKind::Http { status, .. } => Some(status),
            _ => None,
        }
    }

    /// Whether Google rejected the session (HTTP 401 or 403), meaning cached
    /// tokens should be dropped and a fresh login attempted.
    #[must_use]
    pub const fn is_auth_failure(&self) -> bool {
        matches!(self.http_status(), Some(401 | 403))
    }
}

/// HTTP status of `err`, if it is a [`Error`] built from a non-success response.
#[must_use]
pub fn http_status(err: &(dyn StdError + 'static)) -> Option<u16> {
    err.downcast_ref::<Error>().and_then(Error::http_status)
}

/// Whether `err` is a [`Error`] signalling a rejected session (HTTP 401/403).
#[must_use]
pub fn is_auth_failure(err: &(dyn StdError + 'static)) -> bool {
    err.downcast_ref::<Error>()
        .is_some_and(Error::is_auth_failure)
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
            ErrorKind::Http { status, body } if body.is_empty() => {
                write!(f, "Google Play returned HTTP {status}")
            }
            ErrorKind::Http { status, body } => {
                write!(f, "Google Play returned HTTP {status}: {body}")
            }
            ErrorKind::IO(err) => err.fmt(f),
            ErrorKind::Str(err) => err.fmt(f),
            ErrorKind::Other(err) => err.fmt(f),
        }
    }
}
