//! One error type for the whole backend, with messages written for the person using the app.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// No G502 HERO is plugged in (or its settings interface can't be opened).
    NotFound(String),
    /// The USB connection failed mid-conversation; the next command reconnects.
    Connection(String),
    /// The mouse didn't answer in time.
    Timeout { feature: u16, function: u8 },
    /// The mouse answered with an HID++ error code.
    Device { feature: u16, function: u8, code: u8 },
    /// The mouse doesn't have a feature this app needs.
    Unsupported(u16),
    /// A safety check stopped the operation before anything was written.
    Refused(String),
    /// Bad input from the page, a bad backup file, or a reply that makes no sense.
    Invalid(String),
    /// Something failed after a profile write started; the old profile is in `backup`.
    AfterWrite { cause: Box<Error>, backup: String },
}

impl Error {
    /// True when the cached USB connection should be thrown away.
    pub fn drops_connection(&self) -> bool {
        match self {
            Error::Connection(_) | Error::Timeout { .. } => true,
            Error::AfterWrite { cause, .. } => cause.drops_connection(),
            _ => false,
        }
    }
}

fn hidpp_error_name(code: u8) -> &'static str {
    match code {
        0x01 => "unknown error",
        0x02 => "invalid argument",
        0x03 => "out of range",
        0x04 => "hardware error",
        0x05 => "internal error",
        0x06 => "invalid feature index",
        0x07 => "invalid function",
        0x08 => "busy",
        0x09 => "unsupported",
        _ => "unrecognised error",
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotFound(why) => write!(f, "G502 HERO not found. {why}"),
            Error::Connection(why) => write!(f, "Lost the connection to the mouse ({why}). Try again."),
            Error::Timeout { feature, function } => write!(
                f,
                "The mouse stopped answering (feature {feature:04X}, function {function}). Try again."
            ),
            Error::Device { feature, function, code } => write!(
                f,
                "The mouse refused the request: {} (code {code:02X}, feature {feature:04X}, function {function}).",
                hidpp_error_name(*code)
            ),
            Error::Unsupported(feature) => {
                write!(f, "This mouse doesn't support HID++ feature {feature:04X}.")
            }
            Error::Refused(why) => write!(f, "Nothing was written: {why}"),
            Error::Invalid(why) => write!(f, "{why}"),
            Error::AfterWrite { cause, backup } => write!(
                f,
                "{cause} The profile may not have saved correctly. Your previous profile is backed up as {backup}: restore it from Backups."
            ),
        }
    }
}

impl std::error::Error for Error {}

impl From<Error> for String {
    fn from(e: Error) -> String {
        e.to_string()
    }
}
