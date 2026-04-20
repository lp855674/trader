#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[error("{code}: {message}")]
pub struct TerminalError {
    code: String,
    message: String,
}

impl TerminalError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}
