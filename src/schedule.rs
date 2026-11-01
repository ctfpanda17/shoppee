use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::{sleep_until, Instant};

pub fn unix_ms() -> anyhow::Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}

/// Snapshot wall time once, then wait on a monotonic clock. No busy spinning.
/// Accuracy is limited by OS timer resolution, clock sync and scheduler load.
pub async fn wait_until_unix_ms(target: u64) -> anyhow::Result<f64> {
    let now = unix_ms()?;
    let delay = target.saturating_sub(now);
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(delay))
        .ok_or_else(|| anyhow::anyhow!("schedule out of range"))?;
    if delay > 0 {
        sleep_until(deadline).await;
    }
    Ok(now.saturating_sub(target) as f64
        + Instant::now()
            .saturating_duration_since(deadline)
            .as_secs_f64()
            * 1000.0)
}
