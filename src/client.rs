use crate::{
    metrics::Sample,
    model::{PendingConfirmation, Product},
};
use anyhow::{bail, ensure, Context, Result};
use reqwest::{redirect::Policy, StatusCode, Url};
use serde::de::DeserializeOwned;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, SystemTime},
};
use tokio::{
    sync::Mutex,
    time::{sleep_until, Instant},
};

#[derive(Clone, Debug)]
pub struct Config {
    pub min_interval: Duration,
    pub timeout: Duration,
    pub max_attempts: usize,
    pub max_retry_wait: Duration,
    pub max_body_bytes: usize,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            min_interval: Duration::from_millis(250),
            timeout: Duration::from_secs(5),
            max_attempts: 3,
            max_retry_wait: Duration::from_secs(60),
            max_body_bytes: 65536,
        }
    }
}

/// A single client, connection pool and serialized rate gate shared by all clones.
/// This generic JSON contract is for the mock/authorized adapters, not Shopee's private API.
#[derive(Clone)]
pub struct Api {
    client: reqwest::Client,
    base: Url,
    config: Config,
    next: Arc<Mutex<Instant>>,
    samples: Arc<Mutex<Vec<Sample>>>,
    halted: Arc<AtomicBool>,
}
impl Api {
    pub fn new(base: &str, config: Config, authorized_remote: bool) -> Result<Self> {
        let base = Url::parse(base)?;
        ensure!(
            base.username().is_empty() && base.password().is_none(),
            "credentials must not be embedded in URL"
        );
        ensure!(
            base.query().is_none() && base.fragment().is_none(),
            "base URL cannot contain query/fragment"
        );
        let local = matches!(base.host_str(), Some("127.0.0.1" | "[::1]" | "::1"));
        ensure!(
            local || authorized_remote,
            "remote endpoint requires --authorized-remote and a permitted adapter"
        );
        ensure!(
            base.scheme() == "https" || (local && base.scheme() == "http"),
            "remote endpoints require HTTPS"
        );
        ensure!(
            config.max_attempts > 0 && config.max_attempts <= 10,
            "attempts must be 1..=10"
        );
        ensure!(
            !config.min_interval.is_zero() && !config.timeout.is_zero(),
            "interval and timeout must be positive"
        );
        ensure!(
            config.max_retry_wait <= Duration::from_secs(3600),
            "retry budget too large"
        );
        ensure!(config.max_body_bytes > 0, "body limit must be positive");
        let client = reqwest::Client::builder()
            .timeout(config.timeout)
            .connect_timeout(config.timeout)
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(2)
            .tcp_nodelay(true)
            .redirect(Policy::none())
            .user_agent("shoppee-core/0.1 authorized-diagnostics")
            .build()?;
        Ok(Self {
            client,
            base,
            config,
            next: Arc::new(Mutex::new(Instant::now())),
            samples: Default::default(),
            halted: Default::default(),
        })
    }
    pub async fn samples(&self) -> Vec<Sample> {
        self.samples.lock().await.clone()
    }
    pub async fn clear_samples(&self) {
        self.samples.lock().await.clear();
    }
    async fn gate(&self) {
        // Release the mutex during timer waits: a 429 response can extend the
        // deadline while other callers are already queued. Always recheck it.
        loop {
            let mut next = self.next.lock().await;
            let now = Instant::now();
            if *next <= now {
                *next = now + self.config.min_interval;
                return;
            }
            let deadline = *next;
            drop(next);
            sleep_until(deadline).await;
        }
    }
    async fn cooldown(&self, delay: Duration) {
        let mut next = self.next.lock().await;
        *next = (*next).max(Instant::now() + delay);
    }
    fn endpoint(&self, kind: &str, id: &str) -> Result<Url> {
        let mut u = self.base.clone();
        u.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("URL has no path segments"))?
            .pop_if_empty()
            .push(kind)
            .push(id);
        Ok(u)
    }
    async fn get<T: DeserializeOwned>(&self, kind: &str, id: &str) -> Result<T> {
        let url = self.endpoint(kind, id)?;
        for attempt in 0..self.config.max_attempts {
            let start = Instant::now();
            self.gate().await;
            ensure!(
                !self.halted.load(Ordering::SeqCst),
                "client stopped after access denial or excessive server cooldown"
            );
            let sent = Instant::now();
            let mut sample = Sample {
                operation: kind.into(),
                status: None,
                success: false,
                queue_ms: (sent - start).as_secs_f64() * 1000.0,
                headers_ms: 0.0,
                body_ms: 0.0,
                processing_ms: 0.0,
                total_ms: 0.0,
            };
            let result = self.client.get(url.clone()).send().await;
            sample.headers_ms = sent.elapsed().as_secs_f64() * 1000.0;
            let retry;
            let mut server_wait = None;
            let parsed: Result<T> = match result {
                Err(e) => {
                    retry = e.is_timeout() || e.is_connect() || e.is_body();
                    Err(anyhow::anyhow!(e.without_url()))
                }
                Ok(mut r) => {
                    let status = r.status();
                    sample.status = Some(status.as_u16());
                    if matches!(status.as_u16(), 401 | 403) {
                        self.halted.store(true, Ordering::SeqCst);
                    }
                    retry = status == StatusCode::TOO_MANY_REQUESTS
                        || matches!(status.as_u16(), 502..=504);
                    if retry {
                        server_wait = r
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(retry_after);
                    }
                    let body_start = Instant::now();
                    let body: Result<Vec<u8>> = async {
                        let mut bytes = Vec::new();
                        while let Some(chunk) = r.chunk().await? {
                            ensure!(
                                bytes.len().saturating_add(chunk.len())
                                    <= self.config.max_body_bytes,
                                "response too large"
                            );
                            bytes.extend_from_slice(&chunk);
                        }
                        Ok(bytes)
                    }
                    .await;
                    sample.body_ms = body_start.elapsed().as_secs_f64() * 1000.0;
                    let processing = Instant::now();
                    let decoded = match body {
                        Ok(bytes) if status.is_success() => {
                            serde_json::from_slice(&bytes).context("invalid JSON contract")
                        }
                        Ok(_) => Err(anyhow::anyhow!("HTTP {}", status.as_u16())),
                        Err(e) => Err(e),
                    };
                    sample.processing_ms = processing.elapsed().as_secs_f64() * 1000.0;
                    decoded
                }
            };
            sample.success = parsed.is_ok();
            sample.total_ms = start.elapsed().as_secs_f64() * 1000.0;
            self.samples.lock().await.push(sample);
            if parsed.is_ok() {
                return parsed;
            }
            if retry {
                let backoff =
                    Duration::from_millis(200 * (1 << attempt) + rand::random_range(0..100));
                let delay = server_wait.unwrap_or_default().max(backoff);
                // Never truncate Retry-After to send early. Stop if it exceeds our budget.
                if delay > self.config.max_retry_wait {
                    self.halted.store(true, Ordering::SeqCst);
                    bail!("server cooldown exceeds retry budget; stopped");
                }
                self.cooldown(delay).await;
            }
            if !retry || attempt + 1 == self.config.max_attempts {
                return parsed;
            }
        }
        unreachable!()
    }
    pub async fn product(&self, id: &str) -> Result<Product> {
        let p: Product = self.get("products", id).await?;
        ensure!(p.id == id, "product identifier mismatch");
        Ok(p)
    }
    pub async fn quote(
        &self,
        id: &str,
        max_price_minor: u64,
        currency: &str,
    ) -> Result<PendingConfirmation> {
        let quote: PendingConfirmation = self.get("quotes", id).await?;
        quote.validate(id, max_price_minor, currency)?;
        Ok(quote)
    }
    /// Deadline includes pacing, retries, network and parsing. No mutating requests.
    pub async fn prepare(
        &self,
        id: &str,
        max_price_minor: u64,
        currency: &str,
        budget: Duration,
    ) -> Result<PendingConfirmation> {
        tokio::time::timeout(budget, async {
            loop {
                let p = self.product(id).await?;
                ensure!(p.currency == currency, "currency mismatch");
                if p.available {
                    return self.quote(id, max_price_minor, currency).await;
                }
            }
        })
        .await
        .context("monitor deadline exceeded")?
    }
}

pub fn retry_after(value: &str) -> Option<Duration> {
    value
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
        .or_else(|| {
            httpdate::parse_http_date(value)
                .ok()
                .map(|t| t.duration_since(SystemTime::now()).unwrap_or_default())
        })
}
