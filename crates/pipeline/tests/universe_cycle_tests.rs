use pipeline::{CycleDecision, UniverseCandidate, UniverseCycleConfig, run_one_cycle_for_universe};

#[test]
fn selects_high_confidence_top_ranked_symbols() {
    let config = UniverseCycleConfig {
        max_positions: 3,
        min_score: 0.5,
        min_confidence: 0.6,
    };
    let candidates = vec![
        UniverseCandidate {
            symbol: "A".to_string(),
            score: 0.9,
            confidence: 0.8,
        },
        UniverseCandidate {
            symbol: "D".to_string(),
            score: 0.7,
            confidence: 0.5,
        },
        UniverseCandidate {
            symbol: "B".to_string(),
            score: 0.6,
            confidence: 0.7,
        },
        UniverseCandidate {
            symbol: "C".to_string(),
            score: 0.4,
            confidence: 0.9,
        },
        UniverseCandidate {
            symbol: "E".to_string(),
            score: 0.55,
            confidence: 0.65,
        },
        UniverseCandidate {
            symbol: "F".to_string(),
            score: 0.85,
            confidence: 0.95,
        },
    ];

    let result = run_one_cycle_for_universe(&config, &candidates);
    assert_eq!(result.accepted, vec!["A", "F", "B"]);
    assert!(
        result
            .rejected
            .contains(&(String::from("D"), CycleDecision::ConfidenceBelowThreshold))
    );
    assert!(
        result
            .rejected
            .contains(&(String::from("C"), CycleDecision::ScoreBelowThreshold))
    );
    assert!(
        result
            .rejected
            .contains(&(String::from("E"), CycleDecision::MaxPositionsReached))
    );
}
