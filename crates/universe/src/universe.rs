#![forbid(unsafe_code)]

use data::Bar;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum UniverseError {
    #[error("universe symbol must not be empty")]
    EmptySymbol,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UniverseContext {
    pub primary_symbol: String,
    pub bar: Bar,
}

pub trait UniverseSelector: Send + Sync {
    fn select(&self, context: &UniverseContext) -> Result<Vec<String>, UniverseError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticUniverseSelector {
    symbols: Vec<String>,
}

impl StaticUniverseSelector {
    pub fn new(symbols: Vec<String>) -> Self {
        Self { symbols }
    }
}

impl UniverseSelector for StaticUniverseSelector {
    fn select(&self, context: &UniverseContext) -> Result<Vec<String>, UniverseError> {
        if context.primary_symbol.trim().is_empty() {
            return Err(UniverseError::EmptySymbol);
        }
        Ok(self
            .symbols
            .iter()
            .filter(|symbol| symbol.as_str() == context.primary_symbol)
            .cloned()
            .collect())
    }
}

pub fn crate_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}
