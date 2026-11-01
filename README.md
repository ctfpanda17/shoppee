# shoppee — 商品任務控制台

v0.2.0 加入本機圖形操作介面：貼上商品網址、設定時間、倒數、停止任務、查看紀錄與匯出 JSON。UI 完整包在同一個 Rust 執行檔，不需要安裝 Node.js、Python 或前端依賴。

**尚未取得蝦皮買家下單授權介面，未提供真實自動下單／付款。** 蝦皮模式會定時開啟官方商品頁；模擬模式可驗證監測、價格檢查與報價確認。兩種模式明確分開，不會把模擬報價當成真實商品資料。

![本機操作介面](docs/ui-v0.2.0.jpg)

## 使用 UI

下載 [v0.2.0](https://github.com/ctfpanda17/shoppee/releases/tag/v0.2.0) 對應平台的 ZIP 並解壓縮。Windows 雙擊 `shoppee-core.exe` 或 `START_UI.cmd`，程式會啟動本機服務並開啟預設瀏覽器。保留終端機視窗；關閉視窗或 Ctrl+C 即停止服務與排程。

也可在終端機執行：

```sh
./shoppee-core ui
# Windows PowerShell：.\shoppee-core.exe ui
```

1. 貼入 `https://shopee.tw/product/商店編號/商品編號` 或帶有 `-i.商店編號.商品編號` 的完整網址，按「解析」。短網址請先在蝦皮開啟，再複製完整商品網址。
2. 選擇「蝦皮商品頁」或「本機模擬」。蝦皮模式到點只會開啟官方商品頁，規格、數量、訂單與付款都由你在蝦皮操作；預算是備註，不會阻止平台成交。模擬模式固定報價 NT$99.00，預算低於此金額會停止。
3. 設定啟動時間（依電腦本地時區）；留白立即開始，最多未來七天。時間準確性依作業系統排程與電腦時鐘同步而定。
4. 按建立任務，右側顯示倒數與狀態；可停止尚未派發的工作。商品頁已派發開啟後無法以取消按鈕撤回瀏覽器操作。
5. 模擬模式會自動啟動／停止本機 mock，顯示 HTTP P50/P95/P99、最近 512 次嘗試與報價，再由你按「確認模擬結果」。匯出 JSON 包含當次狀態與量測；沒有付款請求。

未提供蝦皮帳密輸入、Cookie 擷取、私有 API、驗證碼處理或安全限制繞過。UI 本機 API 只綁定 127.0.0.1，使用隨機工作階段與 Origin／Host 檢查；不要分享程式印出的完整工作階段網址。程式關閉後設定不會持久保存。`ui --port 8788 --no-open` 可指定本機埠並停用自動開啟操作頁。

## 舊有命令列功能

以下 mock／run／bench／probe 指令保留。直接執行而不加參數，現在會啟動 UI。

## 快速開始

需求：Rust 1.99+、curl 7.70+。測試與 TLS 範例另需 OpenSSL CLI；重現報告需 Python 3。Cargo.lock 已提交。

```sh
cargo build --locked --release
cargo test --locked --all-targets
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
```

第一個終端啟動本機模擬伺服器：

```sh
cargo run --locked --release -- mock --release-after-ms 3000 --delay-ms 10
```

另一個終端執行：

```sh
cargo run --locked --release -- run --product coffee --max-price-minor 10000 --currency TWD
```

金額以最小貨幣單位表示，例如 TWD 9900 表示 NT$99.00。輸出 `awaiting_user_confirmation` 與 30 秒有效的非約束報價。使用者必須在平台正式確認商品、數量、運費、總額及付款；本程式沒有執行付款的方法。報價逾時、價格超標、幣別不符都會失敗。

`--at-unix-ms <UTC epoch milliseconds>` 在預熱讀取後以單調時鐘等到指定時間，再開始監測。`--budget-secs 30` 限制監測全流程。若需要觀察未開賣 → 開賣的轉換，請讓模擬伺服器的 release-after-ms 足夠長。Ctrl+C 可取消等待與網路工作。

## 架構

| 模組 | 行為 |
|---|---|
| `client.rs` | Tokio 非同步 reqwest、單一共享連線池、TCP_NODELAY、連線與整體請求 timeout、讀完 body 促進連線重用 |
| 共用節流器 | 所有 Api clones 共用起始時間閘門；預設至少 250ms 一次，不使用忙迴圈或多 IP／多帳戶規避限制 |
| 重試 | 僅冪等 GET；連線／timeout、429、502–504；最多 3 次，指數退避加 jitter；遵守秒數及 HTTP-date Retry-After |
| 停止條件 | 401/403 停止該 client；異常 JSON、過大 body、其他 4xx 不重試；伺服器等待超過 60 秒預算時停止，不縮短伺服器限制 |
| `schedule.rs` | UTC epoch 轉單調 deadline；記錄喚醒遲到量，不自動提前發送、不保證硬即時精度 |
| `model.rs` | 商品與非約束報價契約、價格／幣別／有效期檢查、強制 requires_user_confirmation |
| `metrics.rs` | 每次嘗試的成功狀態、HTTP code、排隊／headers／body／JSON 處理／總耗時及 nearest-rank P50/P95/P99 |
| `probe.rs` | 隔離的冷連線 curl 診斷；不在搶購熱路徑啟動子程序；不追蹤重導向、驗證 TLS、保留代理環境 |
| `mock.rs` | Axum loopback 模擬、定時開賣、延遲注入、429／503 注入及統計 |

退避時間會進入後續 request 的 queue 指標；workflow_ms 才是整個監測到報價的時間。JSON processing_ms 只量反序列化，並非整個程式 CPU 時間。樣本保存在記憶體，適合有限時段執行；長期服務應接入有界 telemetry sink。

## 模擬故障

```sh
cargo run --release -- mock --rate-limit-first 1 --fail-first 1
```

第一個請求 429（Retry-After: 1），第二個 503，後續成功。`/stats` 顯示 requests、quotes、payment_requests；模擬伺服器沒有付款路由。測試會直接計數已接受的 TCP socket，確認同一 Rust client 的多次請求確實重用連線。

## 效能量測

伺服器啟動後：

```sh
cargo run --locked --release -- bench --samples 100 > benchmark.json
python3 scripts/benchmark.py --samples 100
```

第二個命令會自動啟動／關閉兩組 HTTP mock（0ms、10ms 注入）與一個自簽 TLS OpenSSL 測試伺服器；產生 reports 中的原始樣本、分位數、排程流程及環境資訊。25ms 的取樣間隔僅用於此 loopback 實驗。遠端速率必須符合授權介面的規範。

`bench` 的三個依序測量組別：

1. 新 curl process 的冷連線：分解 DNS、TCP、TLS、request-to-first-byte、body、total。
2. 每次建立 reqwest client 的對照組：量 client 建立時間及 HTTP service 時間。
3. 一次預熱後共用 reqwest client：量 pool 重用時的 service、queue、JSON processing 等時間。

第一次發生冷探測錯誤便停止整個 benchmark，避免忽略 429 後繼續請求；Rust 量測也在錯誤後停止。成功分位數不混入失敗時間；報告保留失敗數及狀態，不能只看成功延遲。

| 指標 | 定義／限制 |
|---|---|
| DNS | curl time_namelookup；可能命中 OS cache；IP literal 不代表真實 DNS 解析延遲 |
| TCP | time_connect − time_namelookup |
| TLS | HTTPS 的 time_appconnect − time_connect；HTTP 為 null，不填 0 |
| HTTP request-to-first-byte | time_starttransfer − time_pretransfer；包含傳輸、排隊、伺服器處理，不能稱為純伺服器 CPU 時間 |
| curl total | time_total；不包含啟動 curl process 的時間；上述各 phase 不一定完整涵蓋 total，另有 pretransfer 等開銷 |
| pooled headers | Rust send 到 headers 完成；包含網路、伺服器與任何重新連線，不虛構其中 DNS／TLS 拆分 |
| service | headers + body + JSON processing；比較 reqwest fresh 與 pooled 時主要看這項 |
| queue / total | queue 是節流／退避等待；total 包含 queue，不能拿它直接與 curl total 比較 |

HTTP client、協定版本、TLS backend、代理和測試順序都會影響結果。fresh 與 pooled 是同一 reqwest 的實用對照，但 OS cache 與時間順序仍是混雜因素；不能把所有差值都歸因於 Rust 或連線池。Raw cold 樣本含 remote_ip、HTTP version、連線數及可用時的 proxy_used。走代理時 DNS/TCP 可能是代理路徑，HTTPS TLS 差值可能包含 CONNECT，並不代表直連原站的各段。

P99 用 nearest-rank，100 筆時是第 99 個排序值，尾端估計仍不穩定；正式比較應在授權配額內增加樣本、交錯／重複測量並比較失敗率。各階段 P95 不可直接相加當成 total P95。

### TLS 階段

`cargo test` 會產生一次性自簽 localhost 憑證並啟動 OpenSSL HTTPS 伺服器，確認未受信任憑證失敗，明確指定測試 CA 後成功且 TLS 耗時大於零。重現腳本提供 100 筆 TLS 分位數。這個 OpenSSL 回應與 Axum HTTP mock 的內容／伺服器不同，不能直接相減推論 TLS 的淨成本。

若有自己的合法授權 HTTPS 測試服務，可使用：

```sh
cargo run --release -- probe --url https://YOUR_AUTHORIZED_HOST/health --authorized-remote --samples 100 --interval-ms 1000
```

不需要 `--insecure`，也沒有忽略 TLS 憑證的選項。`--ca-cert` 僅供明確信任的測試 CA。

## 如何找真正瓶頸

先看失敗率、429、queue 與 schedule_lateness_ms，再分辨 DNS／TCP／TLS／HTTP service 與本機 JSON processing。對照 0ms、10ms 注入的變化可以驗證量測是否能定位伺服器／網路等待。請閱讀 [實測結果](reports/ANALYSIS.md)。本機結果只能證明量測與流程，不代表蝦皮尖峰服務或任何實際搶購成功率。

要比較實際網路路徑，應在允許的不同主機／ISP／地區執行相同版本、參數及授權目標，記錄端點、協定、代理、取樣時段與失敗率，優先比較 service P95/P99。此程式不修改路由、不硬編碼 CDN IP、不競速發送多份下單；目前沒有資料能判定台灣使用者的最佳網路路徑。

## 未來平台整合契約

目前 `/products/{id}`、`/quotes/{id}` 是本專案 mock 的 JSON 格式，**不是蝦皮 API**。正式 adapter 必須由平台核准的 API／SDK 實作，從環境或安全儲存取得權杖、遵守核准配額、對應商品與價格資料、停在使用者確認。不要把私有網頁端點或逆向簽章填入此契約。無授權的正式商品／結帳介面時，不啟用正式交易。

不得自動解 CAPTCHA、模擬通過安全挑戰、規避平台限流或重播付款。新增正式下單功能前需重新設計報價綁定、使用者一次性確認、冪等鍵、重複訂單防護、庫存與價格再驗證，以及敏感資料遮蔽。本版本刻意只有 read-only 準備能力。

參考：[reqwest Client 連線池](https://docs.rs/reqwest/0.12/reqwest/struct.Client.html)、[curl write-out 計時定義](https://curl.se/docs/manpage.html#-w)、[Tokio sleep_until](https://docs.rs/tokio/latest/tokio/time/fn.sleep_until.html)。
