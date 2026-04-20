use domain::Side;

use crate::PipelineError;

const DEFAULT_COOLDOWN_SECS: i64 = 300;

#[derive(Clone)]
pub struct ExecutionGuardInput {
    pub account_id: String,
    pub instrument_id: i64,
    pub symbol: String,
    pub side: Side,
    pub strategy_id: String,
    pub ts_ms: i64,
}

pub enum ExecutionGuardDecision {
    Allow { idempotency_key: String },
    Deny { reason: String },
}

pub async fn evaluate_execution_guard(
    database: &db::Db,
    input: &ExecutionGuardInput,
) -> Result<ExecutionGuardDecision, PipelineError> {
    let cooldown_ms = load_cooldown_ms();
    let idempotency_key = build_idempotency_key(input, cooldown_ms);
    if db::order_exists_by_idempotency_key(database.pool(), &input.account_id, &idempotency_key)
        .await?
    {
        return Ok(ExecutionGuardDecision::Deny {
            reason: "guard_duplicate_idempotency".to_string(),
        });
    }

    if db::has_open_order_for_instrument(database.pool(), &input.account_id, input.instrument_id)
        .await?
    {
        return Ok(ExecutionGuardDecision::Deny {
            reason: "guard_open_order_exists".to_string(),
        });
    }

    if let Some(last_order_ts) = db::latest_order_ts_for_instrument_side(
        database.pool(),
        &input.account_id,
        input.instrument_id,
        side_str(input.side),
    )
    .await?
    {
        if input.ts_ms.saturating_sub(last_order_ts) < cooldown_ms {
            return Ok(ExecutionGuardDecision::Deny {
                reason: "guard_cooldown_active".to_string(),
            });
        }
    }

    if let Some(position) = db::local_position_summary_for_instrument(
        database.pool(),
        &input.account_id,
        input.instrument_id,
    )
    .await?
    {
        let same_direction = (position.net_qty > 0.0 && matches!(input.side, Side::Buy))
            || (position.net_qty < 0.0 && matches!(input.side, Side::Sell));
        if same_direction {
            return Ok(ExecutionGuardDecision::Deny {
                reason: "guard_same_direction_position_open".to_string(),
            });
        }
    }

    Ok(ExecutionGuardDecision::Allow { idempotency_key })
}

fn load_cooldown_ms() -> i64 {
    let cooldown_secs = std::env::var("QUANTD_EXEC_SYMBOL_COOLDOWN_SECS")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value >= 0)
        .unwrap_or(DEFAULT_COOLDOWN_SECS);
    cooldown_secs.saturating_mul(1_000)
}

fn build_idempotency_key(input: &ExecutionGuardInput, cooldown_ms: i64) -> String {
    let bucket_start_ms = bucket_start_ms(input.ts_ms, cooldown_ms);
    format!(
        "cycle:{}:{}:{}:{}:{}",
        input.account_id,
        input.instrument_id,
        side_str(input.side),
        input.strategy_id,
        bucket_start_ms
    )
}

fn bucket_start_ms(ts_ms: i64, cooldown_ms: i64) -> i64 {
    if cooldown_ms <= 0 {
        return ts_ms;
    }
    ts_ms - ts_ms.rem_euclid(cooldown_ms)
}

fn side_str(side: Side) -> &'static str {
    match side {
        Side::Buy => "buy",
        Side::Sell => "sell",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExecutionGuardDecision, ExecutionGuardInput, bucket_start_ms, build_idempotency_key,
        evaluate_execution_guard,
    };
    use domain::Side;

    #[test]
    fn cooldown_bucket_is_stable() {
        assert_eq!(bucket_start_ms(310_000, 300_000), 300_000);
        assert_eq!(bucket_start_ms(599_999, 300_000), 300_000);
    }

    #[test]
    fn idempotency_key_is_stable_within_bucket() {
        let left = ExecutionGuardInput {
            account_id: "acc".to_string(),
            instrument_id: 7,
            symbol: "AAPL.US".to_string(),
            side: Side::Buy,
            strategy_id: "s1".to_string(),
            ts_ms: 310_000,
        };
        let right = ExecutionGuardInput {
            ts_ms: 599_999,
            ..left.clone()
        };
        assert_eq!(
            build_idempotency_key(&left, 300_000),
            build_idempotency_key(&right, 300_000)
        );
    }

    #[tokio::test]
    async fn cooldown_guard_uses_latest_order_timestamp() {
        let database = db::Db::connect("sqlite::memory:").await.expect("db");
        db::ensure_mvp_seed(database.pool()).await.expect("seed");
        let instrument_id = db::upsert_instrument(database.pool(), "US_EQUITY", "AAPL.US")
            .await
            .expect("instrument");

        db::insert_order(
            database.pool(),
            &db::NewOrder {
                order_id: "order-1",
                account_id: "acc_mvp_paper",
                instrument_id,
                side: "buy",
                qty: 1.0,
                status: "FILLED",
                order_type: "limit",
                limit_price: Some(100.0),
                exchange_ref: Some("paper-order-1"),
                idempotency_key: Some("old-key"),
                created_at_ms: 299_999,
                updated_at_ms: 299_999,
            },
        )
        .await
        .expect("insert order");

        let deny = evaluate_execution_guard(
            &database,
            &ExecutionGuardInput {
                account_id: "acc_mvp_paper".to_string(),
                instrument_id,
                symbol: "AAPL.US".to_string(),
                side: Side::Buy,
                strategy_id: "ranked_buy".to_string(),
                ts_ms: 300_000,
            },
        )
        .await
        .expect("deny decision");
        assert!(matches!(
            deny,
            ExecutionGuardDecision::Deny { ref reason } if reason == "guard_cooldown_active"
        ));

        let allow = evaluate_execution_guard(
            &database,
            &ExecutionGuardInput {
                account_id: "acc_mvp_paper".to_string(),
                instrument_id,
                symbol: "AAPL.US".to_string(),
                side: Side::Buy,
                strategy_id: "ranked_buy".to_string(),
                ts_ms: 600_000,
            },
        )
        .await
        .expect("allow decision");
        assert!(matches!(allow, ExecutionGuardDecision::Allow { .. }));
    }

    #[tokio::test]
    async fn same_direction_position_blocks_repeat_add_but_allows_reduce() {
        let database = db::Db::connect("sqlite::memory:").await.expect("db");
        db::ensure_mvp_seed(database.pool()).await.expect("seed");
        let instrument_id = db::upsert_instrument(database.pool(), "US_EQUITY", "AAPL.US")
            .await
            .expect("instrument");

        db::insert_order(
            database.pool(),
            &db::NewOrder {
                order_id: "order-long",
                account_id: "acc_mvp_paper",
                instrument_id,
                side: "buy",
                qty: 1.0,
                status: "FILLED",
                order_type: "limit",
                limit_price: Some(100.0),
                exchange_ref: Some("paper-order-long"),
                idempotency_key: Some("old-key"),
                created_at_ms: 1,
                updated_at_ms: 1,
            },
        )
        .await
        .expect("insert order");
        db::insert_fill(
            database.pool(),
            &db::NewFill {
                fill_id: "fill-long",
                order_id: "order-long",
                qty: 1.0,
                price: 100.0,
                created_at_ms: 1,
            },
        )
        .await
        .expect("insert fill");

        let deny_buy = evaluate_execution_guard(
            &database,
            &ExecutionGuardInput {
                account_id: "acc_mvp_paper".to_string(),
                instrument_id,
                symbol: "AAPL.US".to_string(),
                side: Side::Buy,
                strategy_id: "ranked_buy".to_string(),
                ts_ms: 600_000,
            },
        )
        .await
        .expect("buy decision");
        assert!(matches!(
            deny_buy,
            ExecutionGuardDecision::Deny { ref reason }
                if reason == "guard_same_direction_position_open"
        ));

        let allow_sell = evaluate_execution_guard(
            &database,
            &ExecutionGuardInput {
                account_id: "acc_mvp_paper".to_string(),
                instrument_id,
                symbol: "AAPL.US".to_string(),
                side: Side::Sell,
                strategy_id: "ranked_buy".to_string(),
                ts_ms: 600_000,
            },
        )
        .await
        .expect("sell decision");
        assert!(matches!(allow_sell, ExecutionGuardDecision::Allow { .. }));
    }

    #[tokio::test]
    async fn open_order_blocks_new_execution_attempt() {
        let database = db::Db::connect("sqlite::memory:").await.expect("db");
        db::ensure_mvp_seed(database.pool()).await.expect("seed");
        let instrument_id = db::upsert_instrument(database.pool(), "US_EQUITY", "AAPL.US")
            .await
            .expect("instrument");

        db::insert_order(
            database.pool(),
            &db::NewOrder {
                order_id: "order-open",
                account_id: "acc_mvp_paper",
                instrument_id,
                side: "buy",
                qty: 1.0,
                status: "SUBMITTED",
                order_type: "limit",
                limit_price: Some(100.0),
                exchange_ref: Some("paper-order-open"),
                idempotency_key: Some("open-key"),
                created_at_ms: 1,
                updated_at_ms: 1,
            },
        )
        .await
        .expect("insert order");

        let deny = evaluate_execution_guard(
            &database,
            &ExecutionGuardInput {
                account_id: "acc_mvp_paper".to_string(),
                instrument_id,
                symbol: "AAPL.US".to_string(),
                side: Side::Buy,
                strategy_id: "ranked_buy".to_string(),
                ts_ms: 600_000,
            },
        )
        .await
        .expect("deny decision");
        assert!(matches!(
            deny,
            ExecutionGuardDecision::Deny { ref reason } if reason == "guard_open_order_exists"
        ));
    }
}
