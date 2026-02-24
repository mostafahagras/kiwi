use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub struct CliError {
    pub code: i32,
    pub message: Option<String>,
}

impl CliError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            code: 1,
            message: Some(message.into()),
        }
    }

    pub fn with_code(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: Some(message.into()),
        }
    }

    pub fn silent(code: i32) -> Self {
        Self {
            code,
            message: None,
        }
    }
}

impl Display for CliError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match &self.message {
            Some(msg) => write!(f, "{msg}"),
            None => Ok(()),
        }
    }
}

impl Error for CliError {}

pub type CliResult<T> = Result<T, CliError>;
