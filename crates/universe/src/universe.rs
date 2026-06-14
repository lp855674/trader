#![forbid(unsafe_code)]

use data::Bar;
use std::collections::BTreeSet;
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
    pub available_symbols: Vec<String>,
}

impl UniverseContext {
    pub fn new(primary_symbol: impl Into<String>, bar: Bar) -> Self {
        Self {
            primary_symbol: primary_symbol.into(),
            bar,
            available_symbols: Vec::new(),
        }
    }

    pub fn with_available_symbols(mut self, available_symbols: Vec<String>) -> Self {
        self.available_symbols = available_symbols;
        self
    }
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
        Ok(self.symbols.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UniverseFilter {
    pub include_symbols: Vec<String>,
    pub exclude_symbols: Vec<String>,
    pub symbol_prefixes: Vec<String>,
    pub require_current_data: bool,
    pub max_symbols: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilteredUniverseSelector {
    symbols: Vec<String>,
    filter: UniverseFilter,
}

impl FilteredUniverseSelector {
    pub fn new(symbols: Vec<String>, filter: UniverseFilter) -> Self {
        Self { symbols, filter }
    }
}

impl UniverseSelector for FilteredUniverseSelector {
    fn select(&self, context: &UniverseContext) -> Result<Vec<String>, UniverseError> {
        if context.primary_symbol.trim().is_empty() {
            return Err(UniverseError::EmptySymbol);
        }

        Ok(filter_symbols(&self.symbols, &self.filter, context))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RankedUniverseSelector {
    ranked_symbols: Vec<String>,
    filter: UniverseFilter,
}

impl RankedUniverseSelector {
    pub fn new(ranked_symbols: Vec<String>, filter: UniverseFilter) -> Self {
        Self {
            ranked_symbols,
            filter,
        }
    }
}

impl UniverseSelector for RankedUniverseSelector {
    fn select(&self, context: &UniverseContext) -> Result<Vec<String>, UniverseError> {
        if context.primary_symbol.trim().is_empty() {
            return Err(UniverseError::EmptySymbol);
        }

        let mut selected = filter_symbols(&self.ranked_symbols, &self.filter, context);
        if let Some(max_symbols) = self.filter.max_symbols {
            selected.truncate(max_symbols);
        }
        Ok(selected)
    }
}

fn filter_symbols(
    symbols: &[String],
    filter: &UniverseFilter,
    context: &UniverseContext,
) -> Vec<String> {
    let include_symbols = (!filter.include_symbols.is_empty())
        .then(|| filter.include_symbols.iter().collect::<BTreeSet<_>>());
    let exclude_symbols = filter.exclude_symbols.iter().collect::<BTreeSet<_>>();
    let available_symbols = filter
        .require_current_data
        .then(|| context.available_symbols.iter().collect::<BTreeSet<_>>());

    symbols
        .iter()
        .filter(|symbol| {
            include_symbols
                .as_ref()
                .is_none_or(|included| included.contains(symbol))
        })
        .filter(|symbol| !exclude_symbols.contains(symbol))
        .filter(|symbol| {
            filter.symbol_prefixes.is_empty()
                || filter
                    .symbol_prefixes
                    .iter()
                    .any(|prefix| symbol.starts_with(prefix))
        })
        .filter(|symbol| {
            available_symbols
                .as_ref()
                .is_none_or(|available| available.contains(symbol))
        })
        .cloned()
        .collect()
}
