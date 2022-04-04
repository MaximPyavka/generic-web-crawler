use std::error::Error;
use std::io::Error as _IOError;

use regex::Error as RegexError;

pub enum ProcessorError {
    HtmlParserBuildError(String),
    NothingToCaptureError,
    RegexBuildError(RegexError),
    HTTPRequestError(reqwest::Error),
    ResponseAdoptionError(reqwest::Error),
    IOError(_IOError),
    UrlParseError(url::ParseError),
    AuthenticationError(String),
}

impl From<RegexError> for ProcessorError {
    fn from(error: RegexError) -> Self {
        ProcessorError::RegexBuildError(error)
    }
}

impl From<reqwest::Error> for ProcessorError {
    fn from(error: reqwest::Error) -> Self {
        ProcessorError::HTTPRequestError(error)
    }
}

impl From<_IOError> for ProcessorError {
    fn from(error: _IOError) -> Self {
        ProcessorError::IOError(error)
    }
}

impl From<url::ParseError> for ProcessorError {
    fn from(error: url::ParseError) -> Self {
        ProcessorError::UrlParseError(error)
    }
}


impl std::fmt::Display for ProcessorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessorError::HtmlParserBuildError(pat) => write!(f, "Failed to build HTML parser, by given pattern: {}", pat),
            ProcessorError::NothingToCaptureError => write!(f, "Failed to capture element by given selector"),
            ProcessorError::RegexBuildError(e) => write!(f, "{}", e),
            ProcessorError::HTTPRequestError(e) => write!(f, "{}", e),
            ProcessorError::ResponseAdoptionError(e) => write!(f, "{}", e),
            ProcessorError::IOError(e) => write!(f, "{}", e),
            ProcessorError::UrlParseError(e) => write!(f, "{}", e),
            ProcessorError::AuthenticationError(mess) => write!(f, "Failed to authenticate: {}", mess),
        }
    }
}

impl std::fmt::Debug for ProcessorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <ProcessorError as std::fmt::Display>::fmt(self, f)
    }
}

impl Error for ProcessorError {}