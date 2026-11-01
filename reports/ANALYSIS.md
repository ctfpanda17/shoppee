# 本機實測與瓶頸分析

2026-11-01，Linux x86_64，rustc 1.99.1 (58cf8f9c3 2026-10-13)，release opt-level 3、LTO disabled、codegen-units=1。每個組別 100 筆，僅連線 127.0.0.1，無蝦皮流量。原始資料與環境資訊在同目錄 JSON 檔案。

## 分位數（毫秒）

| 量測 | P50 | P95 | P99 |
|---|---:|---:|---:|
| 0ms 注入：curl cold total | 0.674000 | 1.704000 | 2.725000 |
| 0ms 注入：reqwest fresh service | 0.664660 | 4.562965 | 5.461612 |
| 0ms 注入：reqwest pooled service | 0.460940 | 10.037317 | 84.188495 |
| 10ms 注入：curl DNS 階段（IP literal） | 0.033000 | 0.115000 | 0.562000 |
| 10ms 注入：curl TCP 階段 | 0.108000 | 0.383000 | 0.783000 |
| 10ms 注入：curl request-to-first-byte | 11.424000 | 12.511000 | 16.540000 |
| 10ms 注入：reqwest fresh service | 12.162033 | 17.213044 | 24.740634 |
| 10ms 注入：reqwest pooled service | 11.705470 | 17.152325 | 100.515261 |
| 10ms 注入：reqwest pooled JSON processing | 0.002559 | 0.009483 | 0.011242 |
| 10ms 注入：reqwest pooled queue | 14.192311 | 15.995574 | 27.556722 |
| 獨立 OpenSSL 測試：cold TLS handshake | 4.719000 | 10.628000 | 14.120000 |

HTTP mock、reqwest fresh／pooled 與 TLS probe 共 700 筆量測請求，全部成功；warm-up 與流程驗證另外計數。HTTP mock 的 TLS 欄位為 null；獨立 TLS fixture 才有握手數據。單次排程流程得到 awaiting_user_confirmation，喚醒遲到 1.897ms，開啟監測至待確認報價耗時 263.738ms。這只是一次排程觀測，不是排程抖動的 P99。

## 有數據支持的判斷

1. 增加 10ms 的伺服器延遲注入後，pooled headers P50 增加 11.235ms。量測能把等待定位在 HTTP headers 前，而不是誤認為 Rust JSON 處理變慢。
2. 10ms 注入時，JSON processing P50 為 0.002559ms，HTTP headers P50 為 11.682ms。此案例中反序列化不是主要瓶頸。
3. 25ms 最小請求間隔讓 queue P50 為 14.192ms；預設正式執行間隔為 250ms。這是配額／節流等待，不能當成網路延遲，也不應為了縮短圖表數字而繞過授權配額。
4. fresh 與 pooled 同樣使用 reqwest，service 指標排除了節流 queue；測試另外直接計數 TCP accept，確認多次 Rust 請求只建立一條連線。cold curl 與 pooled reqwest 不能作為單一變因的因果比較。
5. 本次 pooled service 的 P99 在 0ms／10ms 組分別為 84.188ms／100.515ms，高於 fresh 對照組。原始尖峰主要出現在 headers 等待與節流 timer queue，JSON 解析仍很短。這只能定位到等待區段，尚不能區分本機 runtime／OS 排程停頓與伺服器處理；需在專用主機搭配 runtime／CPU telemetry 重跑，不能宣稱連線池在所有尾端分位數都更快。
6. TLS 握手成本只能由獨立 HTTPS fixture 觀測，不能把與 HTTP mock 的 total 差值都算作 TLS。TLS 驗證持續開啟，測試 CA 僅明確提供給本機測試。

## 限制與下一次量測

DNS 使用 IP literal，這裡的 DNS 數值只是 curl 的本地名稱處理階段，沒有測到 ISP resolver 的真實 DNS。loopback 不包含跨網路路由、CDN、平台排隊、尖峰庫存競爭、真實憑證鏈或交易端點；目前無法判定蝦皮的最佳網路路徑或搶購成功率。需要平台正式授權的測試介面後，才能在目標主機／ISP／地區重跑，保留 HTTP 版本、代理、失敗率與取樣時段做比較。

100 筆的 P99 很容易受單筆抖動影響；各組依序執行而非交錯，系統負載與 cache 仍有混雜。HTTP request-to-first-byte 包括傳輸與伺服器等待；本工具未分別測量這兩者。JSON processing 是牆鐘時間，只包含反序列化，不是整個進程 CPU 用量。所有階段分位數不可相加推算 total 分位數。

## 驗證與重現

16 項 integration tests 通過（包含 shared Retry-After 對已排隊 clone 的影響、連線重用、期限、價格／幣別／有效期／確認限制、拒絕存取不重試、受信任／不受信任 TLS 憑證）。cargo fmt 與 Clippy -D warnings 通過，release 建置成功。初期 thin LTO 連結失敗，最終設定已關閉該可選最佳化；此處所有數據由最終成功建置的版本重新量測。

```sh
cargo build --locked --release
python3 scripts/benchmark.py --samples 100
```

`environment.json` 記錄 Cargo.toml、Cargo.lock 及 src/*.rs 的順序與內容 SHA-256：`d35ec786ba2f7413f77a10c8639c5de01b010803ec4b556f8d256a653cc0f419`。這個 hash 不包含測試、文件或報告，避免自我引用。腳本會產生新的資料，數值會隨主機負載改變。
