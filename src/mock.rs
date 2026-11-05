use crate::model::{PendingConfirmation, Product};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::time::Instant;

#[derive(Clone, Debug)]
pub struct MockConfig {
    pub release_after: Duration,
    pub delay: Duration,
    pub fail_first: u64,
    pub rate_limit_first: u64,
    pub retry_after_secs: u64,
}
impl Default for MockConfig {
    fn default() -> Self {
        Self {
            release_after: Duration::from_millis(300),
            delay: Duration::from_millis(10),
            fail_first: 0,
            rate_limit_first: 0,
            retry_after_secs: 1,
        }
    }
}
struct Inner {
    config: MockConfig,
    started: Instant,
    requests: AtomicU64,
    quotes: AtomicU64,
}
#[derive(Clone)]
pub struct MockState(Arc<Inner>);
impl MockState {
    pub fn new(config: MockConfig) -> Self {
        Self(Arc::new(Inner {
            config,
            started: Instant::now(),
            requests: AtomicU64::new(0),
            quotes: AtomicU64::new(0),
        }))
    }
    pub fn requests(&self) -> u64 {
        self.0.requests.load(Ordering::SeqCst)
    }
    pub fn quotes(&self) -> u64 {
        self.0.quotes.load(Ordering::SeqCst)
    }
    async fn guard(&self) -> Result<(), Failure> {
        if !self.0.config.delay.is_zero() {
            tokio::time::sleep(self.0.config.delay).await;
        }
        let n = self.0.requests.fetch_add(1, Ordering::SeqCst);
        let c = &self.0.config;
        let mut h = HeaderMap::new();
        if n < c.rate_limit_first {
            h.insert(
                "retry-after",
                c.retry_after_secs.to_string().parse().unwrap(),
            );
            return Err(Failure(Box::new((
                StatusCode::TOO_MANY_REQUESTS,
                h,
                "rate limited".into(),
            ))));
        }
        if n - c.rate_limit_first < c.fail_first {
            return Err(Failure(Box::new((
                StatusCode::SERVICE_UNAVAILABLE,
                h,
                "temporary failure".into(),
            ))));
        }
        Ok(())
    }
    fn product(&self, id: String) -> Product {
        Product {
            id,
            available: self.0.started.elapsed() >= self.0.config.release_after,
            price_minor: 9900,
            currency: "TWD".into(),
        }
    }
}
struct Failure(Box<(StatusCode, HeaderMap, String)>);
impl axum::response::IntoResponse for Failure {
    fn into_response(self) -> axum::response::Response {
        (*self.0).into_response()
    }
}
async fn product(
    State(s): State<MockState>,
    Path(id): Path<String>,
) -> Result<Json<Product>, Failure> {
    s.guard().await?;
    Ok(Json(s.product(id)))
}
async fn quote(
    State(s): State<MockState>,
    Path(id): Path<String>,
) -> Result<Json<PendingConfirmation>, Failure> {
    s.guard().await?;
    let product = s.product(id);
    if !product.available {
        return Err(Failure(Box::new((
            StatusCode::CONFLICT,
            HeaderMap::new(),
            "not released".into(),
        ))));
    }
    s.0.quotes.fetch_add(1, Ordering::SeqCst);
    Ok(Json(PendingConfirmation {
        product,
        expires_unix_ms: crate::schedule::unix_ms().unwrap() + 30_000,
        requires_user_confirmation: true,
    }))
}
#[derive(Serialize)]
struct Stats {
    requests: u64,
    quotes: u64,
    payment_requests: u64,
}
async fn stats(State(s): State<MockState>) -> Json<Stats> {
    Json(Stats {
        requests: s.requests(),
        quotes: s.quotes(),
        payment_requests: 0,
    })
}
pub fn router(state: MockState) -> Router {
    Router::new()
        .route("/products/{id}", get(product))
        .route("/quotes/{id}", get(quote))
        .route("/stats", get(stats))
        .with_state(state)
}
