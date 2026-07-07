use std::time::Duration;

use crate::ingestion::IngestionError;

#[derive(Debug, Clone, Copy)]
pub struct FetchRetryPolicy {
    pub max_attempts: usize,
    pub initial_backoff: Duration,
}

impl Default for FetchRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(250),
        }
    }
}

pub async fn get_text_with_retry(
    client: &reqwest::Client,
    url: &str,
    query: &[(&str, String)],
) -> Result<String, IngestionError> {
    get_text_with_retry_policy(client, url, query, FetchRetryPolicy::default()).await
}

pub async fn get_text_with_retry_policy(
    client: &reqwest::Client,
    url: &str,
    query: &[(&str, String)],
    policy: FetchRetryPolicy,
) -> Result<String, IngestionError> {
    let max_attempts = policy.max_attempts.max(1);
    let mut backoff = policy.initial_backoff;

    for attempt in 1..=max_attempts {
        match client.get(url).query(query).send().await {
            Ok(response) => {
                let status = response.status();
                if is_retryable_status(status) && attempt < max_attempts {
                    sleep_backoff(backoff).await;
                    backoff = next_backoff(backoff);
                    continue;
                }
                return Ok(response.error_for_status()?.text().await?);
            }
            Err(error) => {
                if is_retryable_error(&error) && attempt < max_attempts {
                    sleep_backoff(backoff).await;
                    backoff = next_backoff(backoff);
                    continue;
                }
                return Err(error.into());
            }
        }
    }

    unreachable!("retry loop always returns on the final attempt")
}

fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn is_retryable_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request()
}

async fn sleep_backoff(backoff: Duration) {
    if !backoff.is_zero() {
        tokio::time::sleep(backoff).await;
    }
}

fn next_backoff(backoff: Duration) -> Duration {
    backoff
        .checked_mul(2)
        .unwrap_or_else(|| Duration::from_secs(u64::MAX))
}
