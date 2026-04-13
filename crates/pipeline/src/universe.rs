#[derive(Clone, Debug)]
pub struct UniverseCandidate {
    pub symbol: String,
    pub score: f64,
    pub confidence: f64,
}

#[derive(Clone, Debug)]
pub struct UniverseCycleConfig {
    pub max_positions: usize,
    pub min_score: f64,
    pub min_confidence: f64,
}

#[derive(Clone, Debug)]
pub struct UniverseCycleResult {
    pub accepted: Vec<String>,
    pub rejected: Vec<(String, CycleDecision)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CycleDecision {
    ScoreBelowThreshold,
    ConfidenceBelowThreshold,
    MaxPositionsReached,
}

impl std::fmt::Display for CycleDecision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CycleDecision::ScoreBelowThreshold => f.write_str("score_below_threshold"),
            CycleDecision::ConfidenceBelowThreshold => f.write_str("confidence_below_threshold"),
            CycleDecision::MaxPositionsReached => f.write_str("max_positions_reached"),
        }
    }
}

impl UniverseCycleResult {
    fn new() -> Self {
        Self {
            accepted: Vec::new(),
            rejected: Vec::new(),
        }
    }
}

pub fn run_one_cycle_for_universe(
    config: &UniverseCycleConfig,
    candidates: &[UniverseCandidate],
) -> UniverseCycleResult {
    let mut result = UniverseCycleResult::new();
    let mut ranked: Vec<&UniverseCandidate> = candidates.iter().collect();
    ranked.sort_by(|a, b| b.score.total_cmp(&a.score));
    for candidate in &ranked {
        if candidate.score < config.min_score {
            result
                .rejected
                .push((candidate.symbol.clone(), CycleDecision::ScoreBelowThreshold));
            continue;
        }
        if candidate.confidence < config.min_confidence {
            result.rejected.push((
                candidate.symbol.clone(),
                CycleDecision::ConfidenceBelowThreshold,
            ));
            continue;
        }
        if result.accepted.len() >= config.max_positions {
            result
                .rejected
                .push((candidate.symbol.clone(), CycleDecision::MaxPositionsReached));
            continue;
        }
        result.accepted.push(candidate.symbol.clone());
    }
    result
}
