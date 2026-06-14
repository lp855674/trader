#![forbid(unsafe_code)]

use alpha::{
    AlphaModel, CompositeAlphaModel, MajorityVoteAlphaModel, NetSignalAlphaModel,
    WeightedAlphaModel,
};
use data::Bar;
use events::{SignalEvent, SignalSide};
use feature_store::{FeatureKey, FeatureRecord, FeatureStore, InMemoryFeatureStore};
use indicators::{
    ExponentialMovingAverage, IndicatorError, RelativeStrengthIndex, SimpleMovingAverage,
};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, VecDeque};
use thiserror::Error;
use universe::{
    FilteredUniverseSelector, RankedUniverseSelector, StaticUniverseSelector, UniverseContext,
    UniverseError, UniverseFilter, UniverseSelector,
};

pub trait Strategy: Send {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyRuntimeMode {
    Backtest,
    Replay,
    Paper,
    Live,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyContext {
    pub strategy_id: String,
    pub symbol: String,
    pub runtime_mode: StrategyRuntimeMode,
}

impl StrategyContext {
    pub fn new(
        strategy_id: impl Into<String>,
        symbol: impl Into<String>,
        runtime_mode: StrategyRuntimeMode,
    ) -> Self {
        Self {
            strategy_id: strategy_id.into(),
            symbol: symbol.into(),
            runtime_mode,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StrategyRegistryError {
    #[error("unknown strategy {0}")]
    UnknownStrategy(String),
    #[error("unknown universe {0}")]
    UnknownUniverse(String),
    #[error("strategy assembly requires at least one symbol")]
    EmptySymbolUniverse,
    #[error("unknown alpha conflict resolution {0}")]
    UnknownAlphaConflictResolution(String),
    #[error("invalid strategy configuration: {0}")]
    InvalidConfig(#[from] StrategyConfigError),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StrategyConfigError {
    #[error("moving average cross windows must be greater than zero")]
    InvalidMovingAverageWindow,
    #[error("relative strength index overbought threshold must be between 1 and 99")]
    InvalidRsiThreshold,
    #[error("alpha component weight must be greater than zero")]
    InvalidAlphaWeight,
    #[error("feature ranked universe requires universe rank config")]
    MissingUniverseRankConfig,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StrategyRegistry;

#[derive(Debug, Clone, PartialEq)]
pub struct StrategyAssemblyConfig {
    pub strategy_name: String,
    pub universe_name: String,
    pub alpha_name: String,
    pub symbols: Vec<String>,
    pub universe_filter: StrategyUniverseFilterConfig,
    pub alpha_components: Vec<StrategyAlphaComponentConfig>,
    pub alpha_conflict_resolution: StrategyAlphaConflictResolution,
    pub alpha_gate: Option<StrategyAlphaGateConfig>,
    pub fast_window: usize,
    pub slow_window: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StrategyAlphaComponentConfig {
    pub name: String,
    pub category: Option<String>,
    pub fast_window: Option<usize>,
    pub slow_window: Option<usize>,
    pub weight: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyAlphaGateConfig {
    pub run_id: String,
    pub feature_name: String,
    pub version: Option<String>,
    pub min_value: Option<Decimal>,
    pub max_value: Option<Decimal>,
    pub records: Vec<FeatureRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrategyUniverseRankConfig {
    pub run_id: String,
    pub feature_name: String,
    pub version: Option<String>,
    pub descending: bool,
    pub records: Vec<FeatureRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrategyAlphaConflictResolution {
    #[default]
    HighestConfidence,
    NetSignal,
    MajorityVote,
    CategoryMajority,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StrategyUniverseFilterConfig {
    pub include_symbols: Vec<String>,
    pub exclude_symbols: Vec<String>,
    pub symbol_prefixes: Vec<String>,
    pub require_current_data: bool,
    pub max_symbols: Option<usize>,
    pub feature_rank: Option<StrategyUniverseRankConfig>,
}

impl From<StrategyUniverseFilterConfig> for UniverseFilter {
    fn from(config: StrategyUniverseFilterConfig) -> Self {
        Self {
            include_symbols: config.include_symbols,
            exclude_symbols: config.exclude_symbols,
            symbol_prefixes: config.symbol_prefixes,
            require_current_data: config.require_current_data,
            max_symbols: config.max_symbols,
        }
    }
}

pub struct StrategyAssembly {
    pub primary_symbol: String,
    pub universe: Box<dyn UniverseSelector>,
    pub alpha: Box<dyn AlphaModel + Send + Sync>,
}

impl StrategyRegistry {
    pub fn create(
        &self,
        name: &str,
        context: StrategyContext,
        fast_window: usize,
        slow_window: usize,
    ) -> Result<Box<dyn Strategy + Send>, StrategyRegistryError> {
        match name {
            "moving_average_cross" => Ok(Box::new(MovingAverageCrossStrategy::from_context(
                context,
                fast_window,
                slow_window,
            )?)),
            "exponential_moving_average_cross" => Ok(Box::new(
                ExponentialMovingAverageCrossStrategy::from_context(
                    context,
                    fast_window,
                    slow_window,
                )?,
            )),
            "price_momentum" => Ok(Box::new(PriceMomentumStrategy::from_context(
                context,
                fast_window,
                slow_window,
            )?)),
            "price_channel_breakout" => Ok(Box::new(PriceChannelBreakoutStrategy::from_context(
                context,
                fast_window,
                slow_window,
            )?)),
            "price_channel_reversion" => Ok(Box::new(PriceChannelReversionStrategy::from_context(
                context,
                fast_window,
                slow_window,
            )?)),
            "relative_strength_index_reversion" => Ok(Box::new(
                RelativeStrengthIndexReversionStrategy::from_context(
                    context,
                    fast_window,
                    slow_window,
                )?,
            )),
            other => Err(StrategyRegistryError::UnknownStrategy(other.to_string())),
        }
    }

    pub fn create_alpha(
        &self,
        name: &str,
        context: StrategyContext,
        fast_window: usize,
        slow_window: usize,
    ) -> Result<Box<dyn AlphaModel + Send + Sync>, StrategyRegistryError> {
        match name {
            "moving_average_cross" => Ok(Box::new(MovingAverageCrossStrategy::from_context(
                context,
                fast_window,
                slow_window,
            )?)),
            "exponential_moving_average_cross" => Ok(Box::new(
                ExponentialMovingAverageCrossStrategy::from_context(
                    context,
                    fast_window,
                    slow_window,
                )?,
            )),
            "price_momentum" => Ok(Box::new(PriceMomentumStrategy::from_context(
                context,
                fast_window,
                slow_window,
            )?)),
            "price_channel_breakout" => Ok(Box::new(PriceChannelBreakoutStrategy::from_context(
                context,
                fast_window,
                slow_window,
            )?)),
            "price_channel_reversion" => Ok(Box::new(PriceChannelReversionStrategy::from_context(
                context,
                fast_window,
                slow_window,
            )?)),
            "relative_strength_index_reversion" => Ok(Box::new(
                RelativeStrengthIndexReversionStrategy::from_context(
                    context,
                    fast_window,
                    slow_window,
                )?,
            )),
            other => Err(StrategyRegistryError::UnknownStrategy(other.to_string())),
        }
    }

    pub fn assemble_alpha(
        &self,
        config: StrategyAssemblyConfig,
        runtime_mode: StrategyRuntimeMode,
    ) -> Result<StrategyAssembly, StrategyRegistryError> {
        let primary_symbol = config
            .symbols
            .first()
            .cloned()
            .ok_or(StrategyRegistryError::EmptySymbolUniverse)?;
        let symbols = config.symbols.clone();
        let universe: Box<dyn UniverseSelector> = match config.universe_name.as_str() {
            "static" => Box::new(StaticUniverseSelector::new(symbols.clone())),
            "filtered" => Box::new(FilteredUniverseSelector::new(
                symbols.clone(),
                config.universe_filter.clone().into(),
            )),
            "ranked" => Box::new(RankedUniverseSelector::new(
                symbols.clone(),
                config.universe_filter.clone().into(),
            )),
            "feature_ranked" => {
                let rank = config
                    .universe_filter
                    .feature_rank
                    .clone()
                    .ok_or(StrategyConfigError::MissingUniverseRankConfig)?;
                Box::new(FeatureRankedUniverseSelector::new(
                    symbols.clone(),
                    config.universe_filter.clone().into(),
                    rank,
                ))
            }
            other => return Err(StrategyRegistryError::UnknownUniverse(other.to_string())),
        };
        let alpha = if symbols.len() == 1 {
            self.create_configured_alpha(&config, &primary_symbol, runtime_mode)?
        } else {
            Box::new(PerSymbolAlphaModel::new(
                self,
                &config,
                runtime_mode,
                symbols,
                primary_symbol.clone(),
            )?)
        };

        Ok(StrategyAssembly {
            primary_symbol,
            universe,
            alpha,
        })
    }

    fn create_configured_alpha(
        &self,
        config: &StrategyAssemblyConfig,
        symbol: &str,
        runtime_mode: StrategyRuntimeMode,
    ) -> Result<Box<dyn AlphaModel + Send + Sync>, StrategyRegistryError> {
        let alpha: Box<dyn AlphaModel + Send + Sync> = if config.alpha_components.is_empty() {
            self.create_alpha(
                &config.alpha_name,
                StrategyContext::new(
                    config.strategy_name.clone(),
                    symbol.to_string(),
                    runtime_mode,
                ),
                config.fast_window,
                config.slow_window,
            )?
        } else {
            match config.alpha_conflict_resolution {
                StrategyAlphaConflictResolution::CategoryMajority => {
                    self.create_category_majority_alpha(config, symbol, runtime_mode)?
                }
                StrategyAlphaConflictResolution::HighestConfidence => {
                    let models =
                        self.create_weighted_alpha_components(config, symbol, runtime_mode)?;
                    Box::new(CompositeAlphaModel::new(models))
                }
                StrategyAlphaConflictResolution::NetSignal => {
                    let models =
                        self.create_weighted_alpha_components(config, symbol, runtime_mode)?;
                    Box::new(NetSignalAlphaModel::new(models))
                }
                StrategyAlphaConflictResolution::MajorityVote => {
                    let models =
                        self.create_weighted_alpha_components(config, symbol, runtime_mode)?;
                    Box::new(MajorityVoteAlphaModel::new(models))
                }
            }
        };
        Ok(apply_alpha_gate(alpha, &config.alpha_gate))
    }

    fn create_weighted_alpha_components(
        &self,
        config: &StrategyAssemblyConfig,
        symbol: &str,
        runtime_mode: StrategyRuntimeMode,
    ) -> Result<Vec<Box<dyn AlphaModel + Send + Sync>>, StrategyRegistryError> {
        let mut models = Vec::<Box<dyn AlphaModel + Send + Sync>>::new();
        for component in &config.alpha_components {
            models.push(self.create_weighted_alpha_component(
                config,
                component,
                symbol,
                runtime_mode,
            )?);
        }
        Ok(models)
    }

    fn create_category_majority_alpha(
        &self,
        config: &StrategyAssemblyConfig,
        symbol: &str,
        runtime_mode: StrategyRuntimeMode,
    ) -> Result<Box<dyn AlphaModel + Send + Sync>, StrategyRegistryError> {
        let mut groups = BTreeMap::<String, Vec<Box<dyn AlphaModel + Send + Sync>>>::new();
        for component in &config.alpha_components {
            let category = alpha_component_category(component);
            let model =
                self.create_weighted_alpha_component(config, component, symbol, runtime_mode)?;
            groups.entry(category).or_default().push(model);
        }

        let category_models = groups
            .into_values()
            .map(|models| {
                Box::new(NetSignalAlphaModel::new(models)) as Box<dyn AlphaModel + Send + Sync>
            })
            .collect();
        Ok(Box::new(MajorityVoteAlphaModel::new(category_models)))
    }

    fn create_weighted_alpha_component(
        &self,
        config: &StrategyAssemblyConfig,
        component: &StrategyAlphaComponentConfig,
        symbol: &str,
        runtime_mode: StrategyRuntimeMode,
    ) -> Result<Box<dyn AlphaModel + Send + Sync>, StrategyRegistryError> {
        if !component.weight.is_finite() || component.weight <= 0.0 {
            return Err(StrategyConfigError::InvalidAlphaWeight.into());
        }
        let model = self.create_alpha(
            &component.name,
            StrategyContext::new(
                config.strategy_name.clone(),
                symbol.to_string(),
                runtime_mode,
            ),
            component.fast_window.unwrap_or(config.fast_window),
            component.slow_window.unwrap_or(config.slow_window),
        )?;
        Ok(Box::new(WeightedAlphaModel::new(model, component.weight)))
    }
}

fn alpha_component_category(component: &StrategyAlphaComponentConfig) -> String {
    component
        .category
        .as_deref()
        .map(str::trim)
        .filter(|category| !category.is_empty())
        .unwrap_or(&component.name)
        .to_string()
}

struct FeatureRankedUniverseSelector {
    symbols: Vec<String>,
    filter: UniverseFilter,
    run_id: String,
    feature_name: String,
    version: Option<String>,
    descending: bool,
    store: InMemoryFeatureStore,
}

impl FeatureRankedUniverseSelector {
    fn new(
        symbols: Vec<String>,
        filter: UniverseFilter,
        config: StrategyUniverseRankConfig,
    ) -> Self {
        let mut store = InMemoryFeatureStore::default();
        for record in config.records {
            store.insert(record);
        }
        Self {
            symbols,
            filter,
            run_id: config.run_id,
            feature_name: config.feature_name,
            version: config.version,
            descending: config.descending,
            store,
        }
    }

    fn latest_rank_value(&self, symbol: &str, ts_ms: i64) -> Option<Decimal> {
        let key = FeatureKey::new(
            self.run_id.clone(),
            symbol.to_string(),
            self.feature_name.clone(),
        );
        self.store
            .range(&key, i64::MIN, ts_ms)
            .into_iter()
            .rfind(|record| {
                self.version
                    .as_deref()
                    .is_none_or(|version| record.version == version)
            })
            .map(|record| record.value)
    }
}

impl UniverseSelector for FeatureRankedUniverseSelector {
    fn select(&self, context: &UniverseContext) -> Result<Vec<String>, UniverseError> {
        let mut ranked = self
            .symbols
            .iter()
            .enumerate()
            .filter_map(|(position, symbol)| {
                self.latest_rank_value(symbol, context.bar.ts_ms)
                    .map(|value| (position, symbol.clone(), value))
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| {
            let value_order = if self.descending {
                right.2.cmp(&left.2)
            } else {
                left.2.cmp(&right.2)
            };
            value_order.then_with(|| left.0.cmp(&right.0))
        });
        let ranked_symbols = ranked
            .into_iter()
            .map(|(_, symbol, _)| symbol)
            .collect::<Vec<_>>();

        RankedUniverseSelector::new(ranked_symbols, self.filter.clone()).select(context)
    }
}

fn apply_alpha_gate(
    alpha: Box<dyn AlphaModel + Send + Sync>,
    gate: &Option<StrategyAlphaGateConfig>,
) -> Box<dyn AlphaModel + Send + Sync> {
    let Some(gate) = gate else {
        return alpha;
    };
    Box::new(FeatureGatedAlphaModel::new(alpha, gate.clone()))
}

struct FeatureGatedAlphaModel {
    model: Box<dyn AlphaModel + Send + Sync>,
    run_id: String,
    feature_name: String,
    version: Option<String>,
    min_value: Option<Decimal>,
    max_value: Option<Decimal>,
    store: InMemoryFeatureStore,
}

impl FeatureGatedAlphaModel {
    fn new(model: Box<dyn AlphaModel + Send + Sync>, config: StrategyAlphaGateConfig) -> Self {
        let mut store = InMemoryFeatureStore::default();
        for record in config.records {
            store.insert(record);
        }
        Self {
            model,
            run_id: config.run_id,
            feature_name: config.feature_name,
            version: config.version,
            min_value: config.min_value,
            max_value: config.max_value,
            store,
        }
    }

    fn pass(&self, signal: &SignalEvent, ts_ms: i64) -> bool {
        let key = FeatureKey::new(
            self.run_id.clone(),
            signal.symbol.clone(),
            self.feature_name.clone(),
        );
        let Some(record) = self
            .store
            .range(&key, i64::MIN, ts_ms)
            .into_iter()
            .rfind(|record| {
                self.version
                    .as_deref()
                    .is_none_or(|version| record.version == version)
            })
        else {
            return false;
        };
        if let Some(min_value) = self.min_value
            && record.value < min_value
        {
            return false;
        }
        if let Some(max_value) = self.max_value
            && record.value > max_value
        {
            return false;
        }
        true
    }

    fn gated(&self, signal: SignalEvent, ts_ms: i64) -> Option<SignalEvent> {
        self.pass(&signal, ts_ms).then_some(signal)
    }
}

impl AlphaModel for FeatureGatedAlphaModel {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        let signal = self.model.on_bar(bar)?;
        self.gated(signal, bar.ts_ms)
    }

    fn on_bar_for_symbol(&mut self, symbol: &str, bar: &Bar) -> Option<SignalEvent> {
        let signal = self.model.on_bar_for_symbol(symbol, bar)?;
        self.gated(signal, bar.ts_ms)
    }
}

struct PerSymbolAlphaModel {
    primary_symbol: String,
    models: BTreeMap<String, Box<dyn AlphaModel + Send + Sync>>,
}

impl PerSymbolAlphaModel {
    fn new(
        registry: &StrategyRegistry,
        config: &StrategyAssemblyConfig,
        runtime_mode: StrategyRuntimeMode,
        symbols: Vec<String>,
        primary_symbol: String,
    ) -> Result<Self, StrategyRegistryError> {
        let mut models = BTreeMap::new();
        for symbol in symbols {
            let model = registry.create_configured_alpha(config, &symbol, runtime_mode)?;
            models.insert(symbol, model);
        }
        Ok(Self {
            primary_symbol,
            models,
        })
    }
}

impl AlphaModel for PerSymbolAlphaModel {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        let primary_symbol = self.primary_symbol.clone();
        self.on_bar_for_symbol(&primary_symbol, bar)
    }

    fn on_bar_for_symbol(&mut self, symbol: &str, bar: &Bar) -> Option<SignalEvent> {
        self.models.get_mut(symbol)?.on_bar_for_symbol(symbol, bar)
    }
}

pub struct MovingAverageCrossStrategy {
    strategy_id: String,
    symbol: String,
    fast_average: SimpleMovingAverage,
    slow_average: SimpleMovingAverage,
    last_side: Option<SignalSide>,
}

impl MovingAverageCrossStrategy {
    pub fn new(
        strategy_id: impl Into<String>,
        symbol: impl Into<String>,
        fast_window: usize,
        slow_window: usize,
    ) -> Result<Self, StrategyConfigError> {
        Ok(Self {
            strategy_id: strategy_id.into(),
            symbol: symbol.into(),
            fast_average: SimpleMovingAverage::new(fast_window).map_err(strategy_config_error)?,
            slow_average: SimpleMovingAverage::new(slow_window).map_err(strategy_config_error)?,
            last_side: None,
        })
    }

    pub fn from_context(
        context: StrategyContext,
        fast_window: usize,
        slow_window: usize,
    ) -> Result<Self, StrategyConfigError> {
        Self::new(
            context.strategy_id,
            context.symbol,
            fast_window,
            slow_window,
        )
    }
}

impl Strategy for MovingAverageCrossStrategy {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        let fast = self.fast_average.update(bar.close);
        let slow = self.slow_average.update(bar.close);
        cross_signal(
            &self.strategy_id,
            &self.symbol,
            &mut self.last_side,
            fast,
            slow,
        )
    }
}

impl AlphaModel for MovingAverageCrossStrategy {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        <Self as Strategy>::on_bar(self, bar)
    }
}

pub struct ExponentialMovingAverageCrossStrategy {
    strategy_id: String,
    symbol: String,
    fast_average: ExponentialMovingAverage,
    slow_average: ExponentialMovingAverage,
    last_side: Option<SignalSide>,
}

impl ExponentialMovingAverageCrossStrategy {
    pub fn new(
        strategy_id: impl Into<String>,
        symbol: impl Into<String>,
        fast_window: usize,
        slow_window: usize,
    ) -> Result<Self, StrategyConfigError> {
        Ok(Self {
            strategy_id: strategy_id.into(),
            symbol: symbol.into(),
            fast_average: ExponentialMovingAverage::new(fast_window)
                .map_err(strategy_config_error)?,
            slow_average: ExponentialMovingAverage::new(slow_window)
                .map_err(strategy_config_error)?,
            last_side: None,
        })
    }

    pub fn from_context(
        context: StrategyContext,
        fast_window: usize,
        slow_window: usize,
    ) -> Result<Self, StrategyConfigError> {
        Self::new(
            context.strategy_id,
            context.symbol,
            fast_window,
            slow_window,
        )
    }
}

impl Strategy for ExponentialMovingAverageCrossStrategy {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        let fast = self.fast_average.update(bar.close);
        let slow = self.slow_average.update(bar.close);
        cross_signal(
            &self.strategy_id,
            &self.symbol,
            &mut self.last_side,
            fast,
            slow,
        )
    }
}

impl AlphaModel for ExponentialMovingAverageCrossStrategy {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        <Self as Strategy>::on_bar(self, bar)
    }
}

pub struct PriceMomentumStrategy {
    strategy_id: String,
    symbol: String,
    fast_window: usize,
    slow_window: usize,
    closes: VecDeque<Decimal>,
    last_side: Option<SignalSide>,
}

impl PriceMomentumStrategy {
    pub fn new(
        strategy_id: impl Into<String>,
        symbol: impl Into<String>,
        fast_window: usize,
        slow_window: usize,
    ) -> Result<Self, StrategyConfigError> {
        if fast_window == 0 || slow_window == 0 {
            return Err(StrategyConfigError::InvalidMovingAverageWindow);
        }
        Ok(Self {
            strategy_id: strategy_id.into(),
            symbol: symbol.into(),
            fast_window,
            slow_window,
            closes: VecDeque::with_capacity(fast_window.max(slow_window) + 1),
            last_side: None,
        })
    }

    pub fn from_context(
        context: StrategyContext,
        fast_window: usize,
        slow_window: usize,
    ) -> Result<Self, StrategyConfigError> {
        Self::new(
            context.strategy_id,
            context.symbol,
            fast_window,
            slow_window,
        )
    }

    fn momentum_values(&self) -> Option<(Decimal, Decimal)> {
        let current_index = self.closes.len().checked_sub(1)?;
        let current = *self.closes.get(current_index)?;
        let fast_index = current_index.checked_sub(self.fast_window)?;
        let slow_index = current_index.checked_sub(self.slow_window)?;
        let fast_base = *self.closes.get(fast_index)?;
        let slow_base = *self.closes.get(slow_index)?;
        let fast_momentum = (current - fast_base) / Decimal::from(self.fast_window);
        let slow_momentum = (current - slow_base) / Decimal::from(self.slow_window);
        Some((fast_momentum, slow_momentum))
    }
}

impl Strategy for PriceMomentumStrategy {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        self.closes.push_back(bar.close);
        let max_window = self.fast_window.max(self.slow_window);
        if self.closes.len() > max_window + 1 {
            self.closes.pop_front();
        }
        let (fast_momentum, slow_momentum) = self.momentum_values()?;
        cross_signal(
            &self.strategy_id,
            &self.symbol,
            &mut self.last_side,
            Some(fast_momentum),
            Some(slow_momentum),
        )
    }
}

impl AlphaModel for PriceMomentumStrategy {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        <Self as Strategy>::on_bar(self, bar)
    }
}

pub struct PriceChannelBreakoutStrategy {
    strategy_id: String,
    symbol: String,
    fast_window: usize,
    slow_window: usize,
    closes: VecDeque<Decimal>,
    last_side: Option<SignalSide>,
}

impl PriceChannelBreakoutStrategy {
    pub fn new(
        strategy_id: impl Into<String>,
        symbol: impl Into<String>,
        fast_window: usize,
        slow_window: usize,
    ) -> Result<Self, StrategyConfigError> {
        if fast_window == 0 || slow_window == 0 {
            return Err(StrategyConfigError::InvalidMovingAverageWindow);
        }
        Ok(Self {
            strategy_id: strategy_id.into(),
            symbol: symbol.into(),
            fast_window,
            slow_window,
            closes: VecDeque::with_capacity(fast_window.saturating_add(slow_window)),
            last_side: None,
        })
    }

    pub fn from_context(
        context: StrategyContext,
        fast_window: usize,
        slow_window: usize,
    ) -> Result<Self, StrategyConfigError> {
        Self::new(
            context.strategy_id,
            context.symbol,
            fast_window,
            slow_window,
        )
    }

    fn breakout_side(&self) -> Option<SignalSide> {
        let required_len = self.fast_window.saturating_add(self.slow_window);
        if self.closes.len() < required_len {
            return None;
        }
        let confirmation_start = self.closes.len().checked_sub(self.fast_window)?;
        let baseline_start = confirmation_start.checked_sub(self.slow_window)?;
        let mut baseline = self
            .closes
            .iter()
            .skip(baseline_start)
            .take(self.slow_window);
        let first = *baseline.next()?;
        let (mut channel_high, mut channel_low) = (first, first);
        for close in baseline {
            channel_high = channel_high.max(*close);
            channel_low = channel_low.min(*close);
        }

        let mut confirmation = self.closes.iter().skip(confirmation_start);
        if confirmation.clone().all(|close| *close > channel_high) {
            return Some(SignalSide::Buy);
        }
        confirmation
            .all(|close| *close < channel_low)
            .then_some(SignalSide::Sell)
    }
}

impl Strategy for PriceChannelBreakoutStrategy {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        self.closes.push_back(bar.close);
        let required_len = self.fast_window.saturating_add(self.slow_window);
        while self.closes.len() > required_len {
            self.closes.pop_front();
        }
        let side = self.breakout_side()?;
        side_signal(&self.strategy_id, &self.symbol, &mut self.last_side, side)
    }
}

impl AlphaModel for PriceChannelBreakoutStrategy {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        <Self as Strategy>::on_bar(self, bar)
    }
}

pub struct PriceChannelReversionStrategy {
    strategy_id: String,
    symbol: String,
    breakout: PriceChannelBreakoutStrategy,
    last_side: Option<SignalSide>,
}

impl PriceChannelReversionStrategy {
    pub fn new(
        strategy_id: impl Into<String>,
        symbol: impl Into<String>,
        fast_window: usize,
        slow_window: usize,
    ) -> Result<Self, StrategyConfigError> {
        let strategy_id = strategy_id.into();
        let symbol = symbol.into();
        Ok(Self {
            breakout: PriceChannelBreakoutStrategy::new(
                strategy_id.clone(),
                symbol.clone(),
                fast_window,
                slow_window,
            )?,
            strategy_id,
            symbol,
            last_side: None,
        })
    }

    pub fn from_context(
        context: StrategyContext,
        fast_window: usize,
        slow_window: usize,
    ) -> Result<Self, StrategyConfigError> {
        Self::new(
            context.strategy_id,
            context.symbol,
            fast_window,
            slow_window,
        )
    }
}

impl Strategy for PriceChannelReversionStrategy {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        let breakout_signal = Strategy::on_bar(&mut self.breakout, bar)?;
        let side = match breakout_signal.side {
            SignalSide::Buy => SignalSide::Sell,
            SignalSide::Sell => SignalSide::Buy,
            SignalSide::CloseLong => SignalSide::CloseShort,
            SignalSide::CloseShort => SignalSide::CloseLong,
        };
        side_signal(&self.strategy_id, &self.symbol, &mut self.last_side, side)
    }
}

impl AlphaModel for PriceChannelReversionStrategy {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        <Self as Strategy>::on_bar(self, bar)
    }
}

pub struct RelativeStrengthIndexReversionStrategy {
    strategy_id: String,
    symbol: String,
    index: RelativeStrengthIndex,
    oversold: Decimal,
    overbought: Decimal,
    last_side: Option<SignalSide>,
}

impl RelativeStrengthIndexReversionStrategy {
    pub fn new(
        strategy_id: impl Into<String>,
        symbol: impl Into<String>,
        period: usize,
        overbought_threshold: usize,
    ) -> Result<Self, StrategyConfigError> {
        if overbought_threshold == 0 || overbought_threshold >= 100 {
            return Err(StrategyConfigError::InvalidRsiThreshold);
        }
        let overbought = Decimal::from(overbought_threshold);
        Ok(Self {
            strategy_id: strategy_id.into(),
            symbol: symbol.into(),
            index: RelativeStrengthIndex::new(period).map_err(strategy_config_error)?,
            oversold: Decimal::from(100) - overbought,
            overbought,
            last_side: None,
        })
    }

    pub fn from_context(
        context: StrategyContext,
        period: usize,
        overbought_threshold: usize,
    ) -> Result<Self, StrategyConfigError> {
        Self::new(
            context.strategy_id,
            context.symbol,
            period,
            overbought_threshold,
        )
    }
}

impl Strategy for RelativeStrengthIndexReversionStrategy {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        let value = self.index.update(bar.close)?;
        let side = if value <= self.oversold {
            SignalSide::Buy
        } else if value >= self.overbought {
            SignalSide::Sell
        } else {
            return None;
        };
        side_signal(&self.strategy_id, &self.symbol, &mut self.last_side, side)
    }
}

impl AlphaModel for RelativeStrengthIndexReversionStrategy {
    fn on_bar(&mut self, bar: &Bar) -> Option<SignalEvent> {
        <Self as Strategy>::on_bar(self, bar)
    }
}

fn cross_signal(
    strategy_id: &str,
    symbol: &str,
    last_side: &mut Option<SignalSide>,
    fast: Option<Decimal>,
    slow: Option<Decimal>,
) -> Option<SignalEvent> {
    let (Some(fast), Some(slow)) = (fast, slow) else {
        return None;
    };
    let side = if fast > slow {
        SignalSide::Buy
    } else if fast < slow {
        SignalSide::Sell
    } else {
        return None;
    };

    side_signal(strategy_id, symbol, last_side, side)
}

fn side_signal(
    strategy_id: &str,
    symbol: &str,
    last_side: &mut Option<SignalSide>,
    side: SignalSide,
) -> Option<SignalEvent> {
    if *last_side == Some(side) {
        return None;
    }
    *last_side = Some(side);

    Some(SignalEvent {
        strategy_id: strategy_id.to_string(),
        symbol: symbol.to_string(),
        side,
        confidence: 0.8,
        ts: chrono::Utc::now(),
    })
}

fn strategy_config_error(error: IndicatorError) -> StrategyConfigError {
    match error {
        IndicatorError::ZeroPeriod => StrategyConfigError::InvalidMovingAverageWindow,
    }
}
