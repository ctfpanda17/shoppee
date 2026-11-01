use anyhow::{ensure, Result};
use clap::{Parser, Subcommand};
use serde_json::json;
use shoppee_core::{
    client::{Api, Config},
    metrics::{percentiles, summarize},
    mock::{self, MockConfig, MockState},
    probe::cold_probe,
    schedule,
};
use std::{net::SocketAddr, path::PathBuf, time::Duration};

#[derive(Parser)]
#[command(
    version,
    about = "Authorized purchase preparation; no order/payment API"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Local-only simulator with deterministic release and fault injection.
    Mock {
        #[arg(long, default_value = "127.0.0.1:8787")]
        bind: SocketAddr,
        #[arg(long, default_value_t = 300)]
        release_after_ms: u64,
        #[arg(long, default_value_t = 10)]
        delay_ms: u64,
        #[arg(long, default_value_t = 0)]
        fail_first: u64,
        #[arg(long, default_value_t = 0)]
        rate_limit_first: u64,
    },
    /// Monitor and prepare a quote. Stops at user confirmation; never pays.
    Run {
        #[arg(long, default_value = "http://127.0.0.1:8787/")]
        base: String,
        #[arg(long, default_value = "demo")]
        product: String,
        #[arg(long, default_value_t = 10000)]
        max_price_minor: u64,
        #[arg(long, default_value = "TWD")]
        currency: String,
        #[arg(long)]
        at_unix_ms: Option<u64>,
        #[arg(long, default_value_t = 250)]
        interval_ms: u64,
        #[arg(long, default_value_t = 30)]
        budget_secs: u64,
        #[arg(long)]
        authorized_remote: bool,
    },
    /// Sequential paced cold curl probes vs pooled Rust requests (JSON report).
    Bench {
        #[arg(long, default_value = "http://127.0.0.1:8787/")]
        base: String,
        #[arg(long, default_value = "demo")]
        product: String,
        #[arg(long, default_value_t = 100)]
        samples: usize,
        #[arg(long, default_value_t = 250)]
        interval_ms: u64,
        #[arg(long)]
        authorized_remote: bool,
    },
    /// Standalone cold phase probe, supports a trusted local test CA.
    Probe {
        #[arg(long)]
        url: String,
        #[arg(long, default_value_t = 100)]
        samples: usize,
        #[arg(long, default_value_t = 250)]
        interval_ms: u64,
        #[arg(long)]
        ca_cert: Option<PathBuf>,
        #[arg(long)]
        authorized_remote: bool,
    },
}
fn print(value: impl serde::Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}
fn bounds(samples: usize, interval: u64) -> Result<()> {
    ensure!((1..=10000).contains(&samples), "samples must be 1..=10000");
    ensure!(
        interval > 0 && interval <= 60000,
        "interval must be 1..=60000ms"
    );
    Ok(())
}
fn cold_summary(v: &[shoppee_core::probe::ColdSample]) -> serde_json::Value {
    json!({"dns_ms":percentiles(v.iter().map(|s|s.dns_ms)),
        "tcp_ms":percentiles(v.iter().map(|s|s.tcp_ms)),
        "tls_ms":percentiles(v.iter().filter_map(|s|s.tls_ms)),
        "request_to_first_byte_ms":percentiles(v.iter().map(|s|s.request_to_first_byte_ms)),
        "body_ms":percentiles(v.iter().map(|s|s.body_ms)),
        "total_ms":percentiles(v.iter().map(|s|s.total_ms))})
}
async fn run() -> Result<()> {
    match Cli::parse().command {
        Commands::Mock {
            bind,
            release_after_ms,
            delay_ms,
            fail_first,
            rate_limit_first,
        } => {
            ensure!(bind.ip().is_loopback(), "mock must bind loopback");
            let listener = tokio::net::TcpListener::bind(bind).await?;
            eprintln!("mock listening on {}", listener.local_addr()?);
            let state = MockState::new(MockConfig {
                release_after: Duration::from_millis(release_after_ms),
                delay: Duration::from_millis(delay_ms),
                fail_first,
                rate_limit_first,
                ..Default::default()
            });
            axum::serve(listener, mock::router(state)).await?;
        }
        Commands::Run {
            base,
            product,
            max_price_minor,
            currency,
            at_unix_ms,
            interval_ms,
            budget_secs,
            authorized_remote,
        } => {
            bounds(1, interval_ms)?;
            ensure!(
                (1..=3600).contains(&budget_secs),
                "budget must be 1..=3600 seconds"
            );
            let api = Api::new(
                &base,
                Config {
                    min_interval: Duration::from_millis(interval_ms),
                    ..Default::default()
                },
                authorized_remote,
            )?;
            // Warm an ordinary read, consuming the same rate budget as other requests.
            api.product(&product).await?;
            let lateness = if let Some(at) = at_unix_ms {
                schedule::wait_until_unix_ms(at).await?
            } else {
                0.0
            };
            let started = tokio::time::Instant::now();
            let quote = api
                .prepare(
                    &product,
                    max_price_minor,
                    &currency,
                    Duration::from_secs(budget_secs),
                )
                .await?;
            print(
                json!({"state":"awaiting_user_confirmation", "quote":quote, "schedule_lateness_ms":lateness,
                "workflow_ms": started.elapsed().as_secs_f64()*1000.0, "network":summarize(&api.samples().await)}),
            )?;
        }
        Commands::Bench {
            base,
            product,
            samples,
            interval_ms,
            authorized_remote,
        } => {
            bounds(samples, interval_ms)?;
            let api = Api::new(
                &base,
                Config {
                    min_interval: Duration::from_millis(interval_ms),
                    max_attempts: 1,
                    ..Default::default()
                },
                authorized_remote,
            )?;
            let mut url = url::Url::parse(&base)?;
            url.path_segments_mut()
                .map_err(|_| anyhow::anyhow!("invalid base"))?
                .pop_if_empty()
                .push("products")
                .push(&product);
            let mut cold = vec![];
            let mut cold_failures = 0;
            for _ in 0..samples {
                match cold_probe(url.as_str(), Duration::from_secs(5), None).await {
                    Ok(s) => cold.push(s),
                    Err(_) => {
                        cold_failures += 1;
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(interval_ms)).await;
            }
            if cold_failures > 0 {
                print(
                    json!({"state":"stopped_on_probe_error", "cold_failures":cold_failures, "cold_raw":cold, "cold_summary":cold_summary(&cold)}),
                )?;
                anyhow::bail!("cold probe failed; benchmark stopped to respect access/rate limits");
            }
            // Same-client control: a new reqwest pool per request, outside the hot path.
            let mut rust_cold = vec![];
            let mut client_setup_ms = vec![];
            for _ in 0..samples {
                let setup = tokio::time::Instant::now();
                let fresh = Api::new(
                    &base,
                    Config {
                        min_interval: Duration::from_millis(interval_ms),
                        max_attempts: 1,
                        ..Default::default()
                    },
                    authorized_remote,
                )?;
                client_setup_ms.push(setup.elapsed().as_secs_f64() * 1000.0);
                fresh.product(&product).await?;
                rust_cold.extend(fresh.samples().await);
                tokio::time::sleep(Duration::from_millis(interval_ms)).await;
            }
            api.product(&product).await?; // excluded warm-up
            api.clear_samples().await;
            for _ in 0..samples {
                if api.product(&product).await.is_err() {
                    break;
                }
            }
            let raw = api.samples().await;
            print(
                json!({"mode":"sequential_cold_curl_vs_warmed_reqwest", "cold_failures":cold_failures,
                "cold_summary":cold_summary(&cold),"pooled_summary":summarize(&raw),"cold_raw":cold,"pooled_raw":raw,
                "reqwest_fresh_summary":summarize(&rust_cold),"reqwest_fresh_raw":rust_cold,
                "client_setup_ms":percentiles(client_setup_ms),
                "notes":["curl vs reqwest is descriptive. Compare reqwest fresh vs pooled to study pool reuse, with temporal order/cache confounds.",
                "HTTP header latency includes network/server time and any pool reconnect; it is not CPU time.",
                "curl subprocess startup is outside curl's transfer timings. DNS may use OS cache.",
                "Proxy routes can include CONNECT/TLS to proxy. Timings do not establish fastest Internet route.",
                "Pooled phase timings are unavailable; TLS on plain HTTP is null.",
                "P99 from 100 samples is exploratory; collect more on an authorized endpoint for stable tails."]}),
            )?;
        }
        Commands::Probe {
            url,
            samples,
            interval_ms,
            ca_cert,
            authorized_remote,
        } => {
            bounds(samples, interval_ms)?;
            // Apply endpoint authorization policy without sending an extra request.
            let _ = Api::new(&url, Config::default(), authorized_remote)?;
            let mut raw = vec![];
            let mut failures = 0;
            for _ in 0..samples {
                match cold_probe(&url, Duration::from_secs(5), ca_cert.as_deref()).await {
                    Ok(s) => raw.push(s),
                    Err(_) => {
                        failures += 1;
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(interval_ms)).await;
            }
            print(
                json!({"requested":samples,"attempts":raw.len()+failures,"failures":failures,"summary":cold_summary(&raw),"raw":raw}),
            )?;
            ensure!(
                failures == 0,
                "probe failed; stopped without sending further requests"
            );
        }
    }
    Ok(())
}
#[tokio::main]
async fn main() -> Result<()> {
    tokio::select! {
        result = run() => result,
        _ = tokio::signal::ctrl_c() => { eprintln!("cancelled"); Ok(()) }
    }
}
