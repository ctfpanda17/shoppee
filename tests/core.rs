use shoppee_core::{
    client::{retry_after, Api, Config},
    metrics::percentiles,
    mock::{self, MockConfig, MockState},
    probe::cold_probe,
};
use std::{
    io,
    net::SocketAddr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::net::{TcpListener, TcpStream};

struct Counted {
    listener: TcpListener,
    accepts: Arc<AtomicUsize>,
}
impl axum::serve::Listener for Counted {
    type Io = TcpStream;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            if let Ok(c) = self.listener.accept().await {
                self.accepts.fetch_add(1, Ordering::SeqCst);
                return c;
            }
            tokio::task::yield_now().await;
        }
    }
    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}
struct Server {
    base: String,
    state: MockState,
    accepts: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn serve(config: MockConfig) -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/", listener.local_addr().unwrap());
    let state = MockState::new(config);
    let accepts = Arc::new(AtomicUsize::new(0));
    let counted = Counted {
        listener,
        accepts: accepts.clone(),
    };
    let app = mock::router(state.clone());
    let task = tokio::spawn(async move {
        axum::serve(counted, app).await.unwrap();
    });
    Server {
        base,
        state,
        accepts,
        task,
    }
}
fn cfg() -> Config {
    Config {
        min_interval: Duration::from_millis(20),
        ..Default::default()
    }
}
#[tokio::test]
async fn full_flow_waits_for_release_reuses_connection_and_stops_before_payment() {
    let s = serve(MockConfig {
        release_after: Duration::from_millis(120),
        ..Default::default()
    })
    .await;
    let api = Api::new(&s.base, cfg(), false).unwrap();
    let quote = api
        .prepare("coffee", 10000, "TWD", Duration::from_secs(3))
        .await
        .unwrap();
    assert!(quote.requires_user_confirmation);
    assert_eq!(s.state.quotes(), 1);
    assert!(s.state.requests() >= 3);
    assert_eq!(
        s.accepts.load(Ordering::SeqCst),
        1,
        "body drain should permit pooled connection reuse"
    );
    let stats: serde_json::Value = reqwest::get(format!("{}stats", s.base))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(stats["payment_requests"], 0);
}
#[tokio::test]
async fn rate_limit_waits_and_retries() {
    let s = serve(MockConfig {
        rate_limit_first: 1,
        release_after: Duration::ZERO,
        ..Default::default()
    })
    .await;
    let api = Api::new(&s.base, cfg(), false).unwrap();
    let start = tokio::time::Instant::now();
    api.product("x").await.unwrap();
    assert!(start.elapsed() >= Duration::from_secs(1));
    let v = api.samples().await;
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].status, Some(429));
    assert!(v[1].success);
}
#[tokio::test]
async fn long_retry_after_stops_without_early_retry() {
    let s = serve(MockConfig {
        rate_limit_first: 5,
        retry_after_secs: 120,
        ..Default::default()
    })
    .await;
    let api = Api::new(&s.base, cfg(), false).unwrap();
    assert!(api
        .product("x")
        .await
        .unwrap_err()
        .to_string()
        .contains("cooldown"));
    assert_eq!(s.state.requests(), 1);
}
#[tokio::test]
async fn transient_failures_are_bounded() {
    let s = serve(MockConfig {
        fail_first: 10,
        ..Default::default()
    })
    .await;
    let api = Api::new(&s.base, cfg(), false).unwrap();
    assert!(api.product("x").await.is_err());
    assert_eq!(s.state.requests(), 3);
}
#[tokio::test]
async fn deadline_cancels_monitor() {
    let s = serve(MockConfig {
        release_after: Duration::from_secs(100),
        ..Default::default()
    })
    .await;
    let api = Api::new(&s.base, cfg(), false).unwrap();
    assert!(api
        .prepare("x", 10000, "TWD", Duration::from_millis(70))
        .await
        .is_err());
    assert_eq!(s.state.quotes(), 0);
}
#[tokio::test]
async fn price_and_currency_guard_quote() {
    let s = serve(MockConfig {
        release_after: Duration::ZERO,
        ..Default::default()
    })
    .await;
    let api = Api::new(&s.base, cfg(), false).unwrap();
    assert!(api.quote("x", 1, "TWD").await.is_err());
    assert!(api.quote("x", 10000, "USD").await.is_err());
}
#[tokio::test]
async fn clones_share_rate_gate() {
    let s = serve(MockConfig::default()).await;
    let api = Api::new(
        &s.base,
        Config {
            min_interval: Duration::from_millis(100),
            ..cfg()
        },
        false,
    )
    .unwrap();
    let a = api.clone();
    let b = api.clone();
    let start = tokio::time::Instant::now();
    let (x, y, z) = tokio::join!(api.product("x"), a.product("y"), b.product("z"));
    x.unwrap();
    y.unwrap();
    z.unwrap();
    assert!(start.elapsed() >= Duration::from_millis(200));
}
#[test]
fn quantiles_and_endpoint_policy() {
    let p = percentiles((1..=100).map(f64::from)).unwrap();
    assert_eq!((p.p50, p.p95, p.p99), (50.0, 95.0, 99.0));
    assert!(percentiles([]).is_none());
    assert!(Api::new("https://example.com/", cfg(), false).is_err());
    assert!(Api::new("http://example.com/", cfg(), true).is_err());
    assert!(Api::new("http://u:p@127.0.0.1/", cfg(), false).is_err());
    assert_eq!(retry_after("2"), Some(Duration::from_secs(2)));
    assert!(retry_after("Sun, 06 Nov 1994 08:49:37 GMT").is_some());
    assert!(retry_after("invalid").is_none());
}
#[tokio::test(start_paused = true)]
async fn schedule_uses_monotonic_wait() {
    let target = shoppee_core::schedule::unix_ms().unwrap() + 500;
    let start = tokio::time::Instant::now();
    let late = shoppee_core::schedule::wait_until_unix_ms(target)
        .await
        .unwrap();
    assert!(start.elapsed() >= Duration::from_millis(490));
    assert!(late < 2.0);
}
#[tokio::test]
async fn cold_http_reports_no_tls() {
    let s = serve(MockConfig::default()).await;
    let p = cold_probe(
        &format!("{}products/x", s.base),
        Duration::from_secs(5),
        None,
    )
    .await
    .unwrap();
    assert!(p.tls_ms.is_none());
    assert_eq!(p.connections, 1);
    assert!(p.total_ms >= p.body_ms);
}
#[tokio::test]
async fn forbidden_and_invalid_json_are_not_retried() {
    for (status, body) in [(403, "captcha required"), (200, "not json")] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let app = axum::Router::new().fallback(move || async move {
            (axum::http::StatusCode::from_u16(status).unwrap(), body)
        });
        let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let api = Api::new(&url, cfg(), false).unwrap();
        assert!(api.product("x").await.is_err());
        assert_eq!(api.samples().await.len(), 1);
        task.abort();
    }
}
#[tokio::test]
async fn oversized_body_and_network_timeout_are_bounded() {
    let s = serve(MockConfig {
        delay: Duration::from_millis(100),
        ..Default::default()
    })
    .await;
    let api = Api::new(
        &s.base,
        Config {
            timeout: Duration::from_millis(10),
            max_attempts: 1,
            ..cfg()
        },
        false,
    )
    .unwrap();
    assert!(api.product("x").await.is_err());
    assert_eq!(api.samples().await.len(), 1);
    let api = Api::new(
        &s.base,
        Config {
            max_body_bytes: 4,
            ..cfg()
        },
        false,
    )
    .unwrap();
    assert!(api.product("x").await.is_err());
}

#[tokio::test]
async fn tls_probe_verifies_certificate_and_records_handshake() {
    use std::process::Stdio;
    let dir = std::env::temp_dir().join(format!("shoppee-tls-{}", rand::random::<u64>()));
    std::fs::create_dir_all(&dir).unwrap();
    let cert = dir.join("cert.pem");
    let key = dir.join("key.pem");
    let output = tokio::process::Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "1",
            "-subj",
            "/CN=127.0.0.1",
            "-addext",
            "subjectAltName=IP:127.0.0.1",
            "-keyout",
        ])
        .arg(&key)
        .arg("-out")
        .arg(&cert)
        .output()
        .await
        .unwrap();
    assert!(output.status.success());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let mut child = tokio::process::Command::new("openssl")
        .args(["s_server", "-accept", &addr.to_string(), "-cert"])
        .arg(&cert)
        .arg("-key")
        .arg(&key)
        .arg("-www")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let ready = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if TcpStream::connect(addr).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    let url = format!("https://{addr}/");
    let untrusted = cold_probe(&url, Duration::from_secs(3), None).await;
    let trusted = cold_probe(&url, Duration::from_secs(3), Some(&cert)).await;
    child.kill().await.unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    ready.unwrap();
    assert!(
        untrusted.is_err(),
        "self-signed certificate must not be trusted implicitly"
    );
    let p = trusted.unwrap();
    assert!(p.tls_ms.unwrap() > 0.0);
    assert_eq!(p.connections, 1);
}

#[test]
fn quote_requires_fresh_expiry_and_confirmation() {
    use shoppee_core::model::{PendingConfirmation, Product};
    let mut quote = PendingConfirmation {
        product: Product {
            id: "x".into(),
            available: true,
            price_minor: 1,
            currency: "TWD".into(),
        },
        expires_unix_ms: 0,
        requires_user_confirmation: true,
    };
    assert!(quote.validate("x", 10, "TWD").is_err());
    quote.expires_unix_ms = shoppee_core::schedule::unix_ms().unwrap() + 10000;
    quote.requires_user_confirmation = false;
    assert!(quote.validate("x", 10, "TWD").is_err());
}
#[tokio::test]
async fn past_schedule_reports_lateness() {
    let target = shoppee_core::schedule::unix_ms().unwrap() - 1000;
    assert!(
        shoppee_core::schedule::wait_until_unix_ms(target)
            .await
            .unwrap()
            >= 1000.0
    );
}

#[tokio::test]
async fn server_cooldown_extends_already_queued_clone() {
    let s = serve(MockConfig {
        rate_limit_first: 1,
        release_after: Duration::ZERO,
        ..Default::default()
    })
    .await;
    let api = Api::new(
        &s.base,
        Config {
            max_attempts: 1,
            min_interval: Duration::from_millis(50),
            ..cfg()
        },
        false,
    )
    .unwrap();
    let clone = api.clone();
    let start = tokio::time::Instant::now();
    let (first, second) = tokio::join!(api.product("x"), clone.product("y"));
    assert!(first.is_err());
    second.unwrap();
    assert!(
        start.elapsed() >= Duration::from_secs(1),
        "queued caller must observe cooldown set by the first response"
    );
    assert_eq!(s.state.requests(), 2);
}
