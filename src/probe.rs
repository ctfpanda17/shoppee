//! Isolated cold-connection diagnostics via curl. Not the production request path.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{path::Path, time::Duration};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColdSample {
    pub status: u16,
    pub dns_ms: f64,
    pub tcp_ms: f64,
    pub tls_ms: Option<f64>,
    pub request_to_first_byte_ms: f64,
    pub body_ms: f64,
    pub total_ms: f64,
    pub connections: u64,
    pub remote_ip: String,
    pub http_version: String,
    pub proxy_used: Option<u64>,
}
#[derive(Deserialize)]
struct CurlTiming {
    http_code: u16,
    time_namelookup: f64,
    time_connect: f64,
    time_appconnect: f64,
    time_pretransfer: f64,
    time_starttransfer: f64,
    time_total: f64,
    num_connects: u64,
    remote_ip: String,
    http_version: String,
    #[serde(default)]
    proxy_used: Option<u64>,
}
/// Fresh curl process: fresh connection pool, but OS DNS/cache and TLS backend state may be warm.
/// Uses inherited proxies and verifies certificates. Never follows redirects.
pub async fn cold_probe(
    url: &str,
    timeout: Duration,
    ca_cert: Option<&Path>,
) -> Result<ColdSample> {
    let u = url::Url::parse(url)?;
    ensure!(matches!(u.scheme(), "http" | "https"), "HTTP(S) only");
    ensure!(
        u.username().is_empty() && u.password().is_none(),
        "URL credentials forbidden"
    );
    let mut cmd = Command::new("curl");
    // --disable must be first: ignore ~/.curlrc (which could enable insecure TLS or redirects).
    cmd.args(["--disable", "--silent", "--show-error", "--output"])
        .arg(if cfg!(windows) { "NUL" } else { "/dev/null" })
        .args(["--write-out", "%{json}", "--max-time"])
        .arg(timeout.as_secs_f64().to_string())
        .args(["--max-filesize", "65536", "--proto", "=http,https"]);
    if let Some(p) = ca_cert {
        cmd.arg("--cacert").arg(p);
    }
    cmd.arg("--url").arg(url).kill_on_drop(true);
    let out = tokio::time::timeout(timeout + Duration::from_secs(2), cmd.output()).await??;
    // Do not echo stderr: URLs may contain sensitive query parameters.
    ensure!(
        out.status.success(),
        "curl diagnostic failed (exit {:?})",
        out.status.code()
    );
    let t: CurlTiming = serde_json::from_slice(&out.stdout).context("curl >=7.70 required")?;
    ensure!(
        (200..300).contains(&t.http_code),
        "probe HTTP {}",
        t.http_code
    );
    let ms = |a: f64, b: f64| (a - b).max(0.0) * 1000.0;
    Ok(ColdSample {
        status: t.http_code,
        dns_ms: t.time_namelookup * 1000.0,
        tcp_ms: ms(t.time_connect, t.time_namelookup),
        tls_ms: (u.scheme() == "https").then(|| ms(t.time_appconnect, t.time_connect)),
        request_to_first_byte_ms: ms(t.time_starttransfer, t.time_pretransfer),
        body_ms: ms(t.time_total, t.time_starttransfer),
        total_ms: t.time_total * 1000.0,
        connections: t.num_connects,
        remote_ip: t.remote_ip,
        http_version: t.http_version,
        proxy_used: t.proxy_used,
    })
}
