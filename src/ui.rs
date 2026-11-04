use crate::{
    browser_bridge::{Broker, Catalog, Kind, Report, Selection},
    client::{Api, Config},
    metrics::{summarize, Sample},
    mock::{self, MockConfig, MockState},
    model::PendingConfirmation,
    product_url::{self, ShopeeProduct},
    schedule,
};
use anyhow::{ensure, Result};
use axum::{
    extract::{DefaultBodyLimit, Request, State},
    http::{header, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::VecDeque, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle};

type Opener = Arc<dyn Fn(&str) -> Result<()> + Send + Sync>;
#[derive(Clone)]
pub struct App {
    inner: Arc<Mutex<Inner>>,
    origin: Arc<str>,
    token: Arc<str>,
    opener: Opener,
    broker: Broker,
}
struct Inner {
    job: Snapshot,
    next_id: u64,
    task: Option<JoinHandle<()>>,
}
#[derive(Debug, Clone, Serialize, Default)]
pub struct Snapshot {
    pub id: u64,
    pub phase: String,
    pub mode: Option<Mode>,
    pub product: Option<ShopeeProduct>,
    pub target_unix_ms: Option<u64>,
    pub schedule_lateness_ms: Option<f64>,
    pub quote: Option<PendingConfirmation>,
    pub logs: VecDeque<Log>,
    pub samples: Vec<Sample>,
    pub request_count: usize,
    pub browser_catalog: Option<Catalog>,
}
#[derive(Debug, Clone, Serialize)]
pub struct Log {
    pub at_unix_ms: u64,
    pub message: String,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Shopee,
    Mock,
    Browser,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartRequest {
    pub url: String,
    pub mode: Mode,
    pub max_price_minor: u64,
    pub at_unix_ms: Option<u64>,
    pub interval_ms: u64,
    pub budget_secs: u64,
    pub mock_release_after_ms: u64,
    #[serde(default)]
    pub selections: Vec<Selection>,
    #[serde(default = "one")]
    pub quantity: u32,
}
fn one() -> u32 {
    1
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParseRequest {
    url: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JobRequest {
    id: u64,
}
#[derive(Debug)]
struct UiError(StatusCode, String);
impl IntoResponse for UiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error":self.1}))).into_response()
    }
}
fn bad(e: impl std::fmt::Display) -> UiError {
    UiError(StatusCode::BAD_REQUEST, e.to_string())
}
impl App {
    pub fn new(
        origin: String,
        token: String,
        opener: impl Fn(&str) -> Result<()> + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                job: Snapshot {
                    phase: "idle".into(),
                    ..Default::default()
                },
                next_id: 1,
                task: None,
            })),
            origin: origin.into(),
            token: token.into(),
            opener: Arc::new(opener),
            broker: Broker::default(),
        }
    }
    async fn update(&self, id: u64, phase: &str, message: &str) {
        let mut inner = self.inner.lock().await;
        if inner.job.id != id {
            return;
        }
        inner.job.phase = phase.into();
        if !message.is_empty() {
            inner.job.logs.push_back(Log {
                at_unix_ms: schedule::unix_ms().unwrap_or_default(),
                message: message.into(),
            });
            while inner.job.logs.len() > 128 {
                inner.job.logs.pop_front();
            }
        }
    }
    async fn record(&self, id: u64, api: &Api) {
        let mut samples = api.samples().await;
        let count = samples.len();
        if count > 512 {
            samples.drain(..count - 512);
        }
        let mut inner = self.inner.lock().await;
        if inner.job.id == id {
            inner.job.samples = samples;
            inner.job.request_count = count;
        }
    }
    pub async fn shutdown(&self) {
        self.broker.cancel().await;
        if let Some(task) = self.inner.lock().await.task.take() {
            task.abort();
        }
    }
}
fn active(phase: &str) -> bool {
    matches!(
        phase,
        "scheduled"
            | "monitoring"
            | "preparing"
            | "opening"
            | "browser_inspecting"
            | "browser_checkout"
    )
}
fn same_token(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |diff, (a, b)| diff | (a ^ b)) == 0
}
async fn guard(State(app): State<App>, req: Request, next: Next) -> Response {
    let host = app.origin.strip_prefix("http://").unwrap_or_default();
    let request_host = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if request_host != host {
        return UiError(StatusCode::FORBIDDEN, "無效的本機主機名稱".into()).into_response();
    }
    if req.uri().path().starts_with("/api/") {
        let token = req
            .headers()
            .get("x-session-token")
            .map(|v| v.as_bytes())
            .unwrap_or_default();
        if !same_token(token, app.token.as_bytes()) {
            return UiError(
                StatusCode::UNAUTHORIZED,
                "工作階段無效，請使用程式啟動時的完整網址".into(),
            )
            .into_response();
        }
        if req.method() != Method::GET {
            let origin = req
                .headers()
                .get(header::ORIGIN)
                .and_then(|v| v.to_str().ok());
            if origin != Some(app.origin.as_ref()) {
                return UiError(StatusCode::FORBIDDEN, "拒絕跨網站操作".into()).into_response();
            }
        }
    }
    let mut response = next.run(req).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response.headers_mut().insert("content-security-policy",HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; base-uri 'none'; frame-ancestors 'none'; form-action 'self'"));
    response
}
async fn index() -> Html<&'static str> {
    Html(include_str!("../ui/index.html"))
}
async fn css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../ui/style.css"),
    )
}
async fn js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../ui/app.js"),
    )
}
async fn state(State(app): State<App>) -> Json<Value> {
    let job = app.inner.lock().await.job.clone();
    Json(
        json!({"version":env!("CARGO_PKG_VERSION"),"capabilities":{"shopee_purchase":false,"shopee_page_open":true,"browser_checkout":true,"mock":true},"metrics":summarize(&job.samples),"job":job}),
    )
}
async fn parse(Json(req): Json<ParseRequest>) -> std::result::Result<Json<ShopeeProduct>, UiError> {
    Ok(Json(product_url::parse(&req.url).map_err(bad)?))
}
async fn start(
    State(app): State<App>,
    Json(req): Json<StartRequest>,
) -> std::result::Result<Json<Value>, UiError> {
    let product = product_url::parse(&req.url).map_err(bad)?;
    let validate = || -> Result<()> {
        ensure!(
            (1..=100_000_000_000).contains(&req.max_price_minor),
            "預算必須大於零且在有效範圍內"
        );
        ensure!(
            (250..=60000).contains(&req.interval_ms),
            "監測間隔必須為 250–60000ms"
        );
        ensure!(
            (1..=3600).contains(&req.budget_secs),
            "監測時限必須為 1–3600 秒"
        );
        ensure!(
            req.mock_release_after_ms <= 60000,
            "模擬開賣等待不可超過 60 秒"
        );
        if let Some(at) = req.at_unix_ms {
            ensure!(
                at <= schedule::unix_ms()?.saturating_add(7 * 86400 * 1000),
                "排程最多可設定未來七天"
            );
        }
        Ok(())
    };
    validate().map_err(bad)?;
    let mut inner = app.inner.lock().await;
    if active(&inner.job.phase) {
        return Err(UiError(
            StatusCode::CONFLICT,
            "已有執行中的工作，請先停止".into(),
        ));
    }
    if let Some(task) = inner.task.take() {
        task.abort();
    }
    let catalog = inner.job.browser_catalog.clone();
    if req.mode == Mode::Browser {
        ensure_browser_start(&req, &product, catalog.as_ref()).map_err(bad)?;
    }
    let id = inner.next_id;
    inner.next_id += 1;
    inner.job = Snapshot {
        id,
        phase: "scheduled".into(),
        mode: Some(req.mode),
        product: Some(product.clone()),
        target_unix_ms: req.at_unix_ms,
        browser_catalog: catalog,
        ..Default::default()
    };
    let task_app = app.clone();
    inner.task = Some(tokio::spawn(async move {
        if let Err(e) = execute(task_app.clone(), id, req, product).await {
            task_app
                .update(id, "failed", &format!("工作停止：{e}"))
                .await;
        }
    }));
    Ok(Json(json!({"id":id})))
}
async fn cancel(
    State(app): State<App>,
    Json(req): Json<JobRequest>,
) -> std::result::Result<Json<Value>, UiError> {
    let mut inner = app.inner.lock().await;
    if inner.job.id != req.id {
        return Err(UiError(
            StatusCode::CONFLICT,
            "工作已更換，請重新整理".into(),
        ));
    }
    if !active(&inner.job.phase) {
        return Err(UiError(StatusCode::CONFLICT, "目前沒有等待中的工作".into()));
    }
    if let Some(task) = inner.task.take() {
        task.abort();
    }
    inner.job.phase = "cancelled".into();
    inner.job.logs.push_back(Log {
        at_unix_ms: schedule::unix_ms().unwrap_or_default(),
        message: "使用者已停止工作".into(),
    });
    drop(inner);
    app.broker.cancel().await;
    Ok(Json(json!({"cancelled":true})))
}
async fn confirm(
    State(app): State<App>,
    Json(req): Json<JobRequest>,
) -> std::result::Result<Json<Value>, UiError> {
    let mut inner = app.inner.lock().await;
    if inner.job.id != req.id
        || inner.job.mode != Some(Mode::Mock)
        || inner.job.phase != "awaiting_confirmation"
    {
        return Err(UiError(
            StatusCode::CONFLICT,
            "只有待確認的模擬結果可以執行此操作".into(),
        ));
    }
    let quote = inner
        .job
        .quote
        .as_ref()
        .ok_or_else(|| bad("沒有有效報價"))?;
    quote
        .validate(&quote.product.id, quote.product.price_minor, "TWD")
        .map_err(bad)?;
    inner.job.phase = "simulation_confirmed".into();
    inner.job.logs.push_back(Log {
        at_unix_ms: schedule::unix_ms().unwrap_or_default(),
        message: "已確認模擬結果；沒有建立真實訂單或付款".into(),
    });
    Ok(Json(json!({"confirmed":true,"payment_sent":false})))
}
struct AbortTask(JoinHandle<()>);
impl Drop for AbortTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}
async fn execute(app: App, id: u64, req: StartRequest, product: ShopeeProduct) -> Result<()> {
    app.update(
        id,
        "scheduled",
        match req.mode {
            Mode::Shopee => "已排定開啟官方商品頁；未啟用自動下單",
            Mode::Mock => "已排定本機模擬；不會連線蝦皮",
            Mode::Browser => "已排定瀏覽器規格選取與前往結帳；不送出訂單或付款",
        },
    )
    .await;
    let late = if let Some(at) = req.at_unix_ms {
        schedule::wait_until_unix_ms(at).await?
    } else {
        0.0
    };
    {
        let mut inner = app.inner.lock().await;
        if inner.job.id != id {
            return Ok(());
        }
        inner.job.schedule_lateness_ms = Some(late);
    }
    if req.mode == Mode::Browser {
        app.update(
            id,
            "browser_checkout",
            "正在選取規格、檢查商品小計與配送；只前往結帳，不送出訂單",
        )
        .await;
        let result = app
            .broker
            .dispatch(
                Kind::Checkout,
                product.canonical_url.clone(),
                req.selections.clone(),
                req.quantity,
                req.max_price_minor,
            )
            .await?;
        {
            let mut inner = app.inner.lock().await;
            if inner.job.id == id {
                inner.job.browser_catalog = Some(result.catalog);
            }
        }
        app.update(
            id,
            "checkout_ready",
            "已前往蝦皮結帳頁，請核對含運總額並親自確認下單與付款",
        )
        .await;
        return Ok(());
    }

    if req.mode == Mode::Shopee {
        app.update(id, "opening", "正在開啟官方商品頁").await;
        let opener = app.opener.clone();
        let url = product.canonical_url.clone();
        tokio::task::spawn_blocking(move || opener(&url)).await??;
        app.update(
            id,
            "opened",
            "已開啟蝦皮官方商品頁，請在蝦皮完成規格選擇、下單與付款確認",
        )
        .await;
        return Ok(());
    }
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}/", listener.local_addr()?);
    let mock_state = MockState::new(MockConfig {
        release_after: Duration::from_millis(req.mock_release_after_ms),
        ..Default::default()
    });
    let _mock = AbortTask(tokio::spawn(async move {
        let _ = axum::serve(listener, mock::router(mock_state)).await;
    }));
    let api = Api::new(
        &base,
        Config {
            min_interval: Duration::from_millis(req.interval_ms),
            ..Default::default()
        },
        false,
    )?;
    app.update(id, "monitoring", "本機模擬已啟動，正在等待模擬商品開賣")
        .await;
    let result = tokio::time::timeout(Duration::from_secs(req.budget_secs), async {
        loop {
            let p = api.product(&product.item_id.to_string()).await?;
            app.record(id, &api).await;
            if p.available {
                app.update(id, "preparing", "模擬商品已開賣，正在檢查預算與報價")
                    .await;
                let quote = api.quote(&p.id, req.max_price_minor, "TWD").await?;
                return Ok::<_, anyhow::Error>(quote);
            }
        }
    })
    .await;
    app.record(id, &api).await;
    let quote = result.map_err(|_| anyhow::anyhow!("已超過監測時限"))??;
    {
        let mut inner = app.inner.lock().await;
        if inner.job.id == id {
            inner.job.quote = Some(quote);
        }
    }
    app.update(
        id,
        "awaiting_confirmation",
        "模擬報價已就緒，等待使用者確認；沒有付款請求",
    )
    .await;
    Ok(())
}

fn ensure_browser_start(
    req: &StartRequest,
    product: &ShopeeProduct,
    catalog: Option<&Catalog>,
) -> Result<()> {
    ensure!((1..=100).contains(&req.quantity), "數量必須為 1–100");
    let catalog = catalog.ok_or_else(|| anyhow::anyhow!("請先連接瀏覽器並讀取規格"))?;
    catalog.validate(&product.canonical_url)?;
    catalog.validate_selection(&req.selections)?;
    ensure!(
        !catalog.shipping_blocked,
        "目前預設地址／門市不適用配送，請先在蝦皮處理後重新讀取規格"
    );
    Ok(())
}
async fn inspect_browser(
    State(app): State<App>,
    Json(req): Json<ParseRequest>,
) -> std::result::Result<Json<Value>, UiError> {
    let product = product_url::parse(&req.url).map_err(bad)?;
    let mut inner = app.inner.lock().await;
    if active(&inner.job.phase) {
        return Err(UiError(StatusCode::CONFLICT, "請先停止目前的工作".into()));
    }
    let id = inner.next_id;
    inner.next_id += 1;
    if let Some(task) = inner.task.take() {
        task.abort();
    }
    inner.job = Snapshot {
        id,
        phase: "browser_inspecting".into(),
        mode: Some(Mode::Browser),
        product: Some(product.clone()),
        ..Default::default()
    };
    let task_app = app.clone();
    inner.task = Some(tokio::spawn(async move {
        let result = task_app
            .broker
            .dispatch(Kind::Inspect, product.canonical_url, vec![], 1, 0)
            .await;
        match result {
            Ok(outcome) => {
                let blocked = outcome.catalog.shipping_blocked;
                {
                    let mut inner = task_app.inner.lock().await;
                    if inner.job.id != id {
                        return;
                    }
                    inner.job.browser_catalog = Some(outcome.catalog);
                }
                task_app
                    .update(
                        id,
                        "catalog_ready",
                        if blocked {
                            "已讀取真實規格，但目前預設地址／門市不適用配送"
                        } else {
                            "已讀取商品規格，請選擇規格、數量與商品小計上限"
                        },
                    )
                    .await;
            }
            Err(e) => {
                task_app
                    .update(id, "failed", &format!("瀏覽器操作停止：{e}"))
                    .await
            }
        }
    }));
    Ok(Json(json!({"id":id})))
}
async fn browser_next(State(app): State<App>) -> Json<Value> {
    Json(json!({"command":app.broker.claim().await}))
}
async fn browser_report(
    State(app): State<App>,
    Json(report): Json<Report>,
) -> std::result::Result<Json<Value>, UiError> {
    app.broker.report(report).await.map_err(bad)?;
    Ok(Json(json!({"accepted":true})))
}

pub fn router(app: App) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/style.css", get(css))
        .route("/app.js", get(js))
        .route("/api/state", get(state))
        .route("/api/export", get(state))
        .route("/api/parse", post(parse))
        .route("/api/browser/inspect", post(inspect_browser))
        .route("/api/browser/next", post(browser_next))
        .route("/api/browser/report", post(browser_report))
        .route("/api/start", post(start))
        .route("/api/cancel", post(cancel))
        .route("/api/confirm", post(confirm))
        .layer(DefaultBodyLimit::max(32768))
        .layer(middleware::from_fn_with_state(app.clone(), guard))
        .with_state(app)
}
pub async fn serve(port: u16, open_browser: bool) -> Result<()> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let origin = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
    let token = rand::random::<[u8; 32]>()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let url = format!("{origin}/#{token}");
    let app = App::new(origin, token, |url| {
        webbrowser::open(url).map_err(Into::into)
    });
    eprintln!("本機操作介面：{url}\n關閉此終端機或按 Ctrl+C 可停止程式。");
    if open_browser {
        tokio::task::spawn_blocking(move || {
            if webbrowser::open(&url).is_err() {
                eprintln!("無法自動開啟瀏覽器，請手動開啟上方完整網址。");
            }
        });
    }
    axum::serve(listener, router(app)).await?;
    Ok(())
}
