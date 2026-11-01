use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use shoppee_core::{
    product_url,
    ui::{router, App},
};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::net::TcpListener;

struct Server {
    base: String,
    app: App,
    client: Client,
    opened: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn server() -> Server {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let opened = Arc::new(Mutex::new(vec![]));
    let output = opened.clone();
    let app = App::new(base.clone(), "test-token".into(), move |url| {
        output.lock().unwrap().push(url.into());
        Ok(())
    });
    let routes = router(app.clone());
    let task = tokio::spawn(async move { axum::serve(listener, routes).await.unwrap() });
    Server {
        base,
        app,
        client: Client::new(),
        opened,
        task,
    }
}
impl Server {
    async fn get(&self, path: &str) -> reqwest::Response {
        self.client
            .get(format!("{}{path}", self.base))
            .header("x-session-token", "test-token")
            .send()
            .await
            .unwrap()
    }
    async fn post(&self, path: &str, data: Value) -> reqwest::Response {
        self.client
            .post(format!("{}{path}", self.base))
            .header("x-session-token", "test-token")
            .header("origin", &self.base)
            .json(&data)
            .send()
            .await
            .unwrap()
    }
    async fn snapshot(&self) -> Value {
        self.get("/api/state").await.json().await.unwrap()
    }
    async fn wait(&self, phase: &str) -> Value {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let data = self.snapshot().await;
                if data["job"]["phase"] == phase {
                    break data;
                }
                assert_ne!(data["job"]["phase"], "failed", "{data}");
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap()
    }
}
fn request(mode: &str) -> Value {
    json!({"url":"https://shopee.tw/coffee-i.123.456?tracking=removed","mode":mode,"max_price_minor":10000,"at_unix_ms":null,"interval_ms":250,"budget_secs":5,"mock_release_after_ms":20})
}
#[test]
fn canonicalizes_product_urls_and_rejects_untrusted_input() {
    let p = product_url::parse("https://shopee.tw/coffee-i.123.456?tracking=x#x").unwrap();
    assert_eq!(p.canonical_url, "https://shopee.tw/product/123/456");
    assert_eq!(
        product_url::parse("https://www.shopee.tw/product/123/456/").unwrap(),
        p
    );
    for s in [
        "https://shopee.tw.evil.test/product/1/2",
        "http://shopee.tw/product/1/2",
        "https://u:p@shopee.tw/product/1/2",
        "https://shopee.tw:444/product/1/2",
        "javascript:alert(1)",
        "https://shopee.tw/product/1/2/3",
        "https://shopee.tw/product/0/2",
        "https://shopee.tw/product/-1/2",
        "https://shopee.tw/foo-i.1.2/3",
        "https://shopee.tw/product/1/18446744073709551616",
        "https://s.shopee.tw/abc",
    ] {
        assert!(product_url::parse(s).is_err(), "{s}");
    }
}
#[tokio::test]
async fn serves_embedded_ui_and_protects_session_origin_and_host() {
    let s = server().await;
    let html = s.get("/").await;
    assert_eq!(html.status(), StatusCode::OK);
    assert!(html.headers().contains_key("content-security-policy"));
    assert!(html.text().await.unwrap().contains("商品任務"));
    for path in ["/app.js", "/style.css"] {
        assert_eq!(s.get(path).await.status(), StatusCode::OK);
    }
    let bare = s
        .client
        .get(format!("{}/api/state", s.base))
        .send()
        .await
        .unwrap();
    assert_eq!(bare.status(), StatusCode::UNAUTHORIZED);
    let foreign = s
        .client
        .post(format!("{}/api/start", s.base))
        .header("x-session-token", "test-token")
        .header("origin", "https://evil.test")
        .json(&request("mock"))
        .send()
        .await
        .unwrap();
    assert_eq!(foreign.status(), StatusCode::FORBIDDEN);
    let missing = s
        .client
        .post(format!("{}/api/start", s.base))
        .header("x-session-token", "test-token")
        .json(&request("mock"))
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::FORBIDDEN);
    let host = s
        .client
        .get(format!("{}/", s.base))
        .header("host", "evil.test")
        .send()
        .await
        .unwrap();
    assert_eq!(host.status(), StatusCode::FORBIDDEN);
    assert_eq!(s.snapshot().await["capabilities"]["shopee_purchase"], false);
    s.app.shutdown().await;
}
#[tokio::test]
async fn mock_flow_quotes_confirms_and_exports_without_opening_shopee() {
    let s = server().await;
    let response = s.post("/api/start", request("mock")).await;
    assert_eq!(response.status(), StatusCode::OK);
    let snapshot = s.wait("awaiting_confirmation").await;
    assert_eq!(snapshot["job"]["quote"]["product"]["price_minor"], 9900);
    assert!(snapshot["job"]["request_count"].as_u64().unwrap() >= 2);
    assert!(s.opened.lock().unwrap().is_empty());
    let id = snapshot["job"]["id"].clone();
    assert_eq!(
        s.post("/api/confirm", json!({"id":99999})).await.status(),
        StatusCode::CONFLICT
    );
    let result: Value = s
        .post("/api/confirm", json!({"id":id}))
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(result["payment_sent"], false);
    let report: Value = s.get("/api/export").await.json().await.unwrap();
    assert_eq!(report["job"]["phase"], "simulation_confirmed");
    assert_eq!(
        s.post("/api/confirm", json!({"id":id})).await.status(),
        StatusCode::CONFLICT
    );
    s.app.shutdown().await;
}
#[tokio::test]
async fn real_mode_opens_only_canonical_page_and_cannot_confirm_payment() {
    let s = server().await;
    s.post("/api/start", request("shopee"))
        .await
        .error_for_status()
        .unwrap();
    let state = s.wait("opened").await;
    assert_eq!(
        *s.opened.lock().unwrap(),
        vec!["https://shopee.tw/product/123/456".to_string()]
    );
    assert_eq!(state["job"]["request_count"], 0);
    assert!(state["job"]["quote"].is_null());
    assert_eq!(
        s.post("/api/confirm", json!({"id":state["job"]["id"]}))
            .await
            .status(),
        StatusCode::CONFLICT
    );
    s.app.shutdown().await;
}
#[tokio::test]
async fn cancellation_stops_schedule_and_rejects_duplicate_jobs() {
    let s = server().await;
    let mut req = request("shopee");
    req["at_unix_ms"] = json!(shoppee_core::schedule::unix_ms().unwrap() + 350);
    let id: Value = s
        .post("/api/start", req.clone())
        .await
        .json()
        .await
        .unwrap();
    assert_eq!(
        s.post("/api/start", req).await.status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        s.post("/api/cancel", json!({"id":id["id"]})).await.status(),
        StatusCode::OK
    );
    tokio::time::sleep(Duration::from_millis(450)).await;
    assert!(s.opened.lock().unwrap().is_empty());
    assert_eq!(s.snapshot().await["job"]["phase"], "cancelled");
    s.app.shutdown().await;
}
#[tokio::test]
async fn invalid_settings_and_price_caps_fail_safely() {
    let s = server().await;
    for (key, value) in [
        ("interval_ms", json!(0)),
        ("budget_secs", json!(0)),
        ("max_price_minor", json!(0)),
        ("mode", json!("auto_purchase")),
        ("at_unix_ms", json!(u64::MAX)),
    ] {
        let mut req = request("mock");
        req[key] = value;
        assert!(s.post("/api/start", req).await.status().is_client_error());
    }
    let mut req = request("mock");
    req["max_price_minor"] = json!(1);
    s.post("/api/start", req).await.error_for_status().unwrap();
    let state = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let data = s.snapshot().await;
            if data["job"]["phase"] == "failed" {
                break data;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .unwrap();
    assert!(state["job"]["quote"].is_null());
    assert!(s.opened.lock().unwrap().is_empty());
    s.app.shutdown().await;
}
