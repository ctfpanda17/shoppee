use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    pub operation: String,
    pub status: Option<u16>,
    pub success: bool,
    pub queue_ms: f64,
    pub headers_ms: f64,
    pub body_ms: f64,
    pub processing_ms: f64,
    pub total_ms: f64,
}

#[derive(Debug, Serialize)]
pub struct Percentiles {
    pub n: usize,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
}

/// Nearest-rank quantiles; unavailable phases are omitted, never filled with zero.
pub fn percentiles(values: impl IntoIterator<Item = f64>) -> Option<Percentiles> {
    let mut v: Vec<_> = values
        .into_iter()
        .filter(|x| x.is_finite() && *x >= 0.0)
        .collect();
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let q = |p: f64| v[((v.len() as f64 * p).ceil() as usize).saturating_sub(1)];
    Some(Percentiles {
        n: v.len(),
        p50: q(0.5),
        p95: q(0.95),
        p99: q(0.99),
    })
}

#[derive(Debug, Serialize)]
pub struct Summary {
    pub attempts: usize,
    pub failures: usize,
    pub statuses: BTreeMap<String, usize>,
    pub successful_ms: BTreeMap<String, Percentiles>,
}

pub fn summarize(samples: &[Sample]) -> Summary {
    let mut statuses = BTreeMap::new();
    for s in samples {
        *statuses
            .entry(
                s.status
                    .map(|x| x.to_string())
                    .unwrap_or("transport_error".into()),
            )
            .or_default() += 1;
    }
    let mut phases = BTreeMap::new();
    for (name, get) in [
        ("queue", (|s: &Sample| s.queue_ms) as fn(&Sample) -> f64),
        ("headers", |s: &Sample| s.headers_ms),
        ("service", |s: &Sample| {
            s.headers_ms + s.body_ms + s.processing_ms
        }),
        ("body", |s: &Sample| s.body_ms),
        ("processing", |s: &Sample| s.processing_ms),
        ("total", |s: &Sample| s.total_ms),
    ] {
        if let Some(p) = percentiles(samples.iter().filter(|s| s.success).map(get)) {
            phases.insert(name.into(), p);
        }
    }
    Summary {
        attempts: samples.len(),
        failures: samples.iter().filter(|s| !s.success).count(),
        statuses,
        successful_ms: phases,
    }
}
