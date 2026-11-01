# shoppee-core v0.1.0

首次發布 Rust 核心與本機模擬驗證版本。

- Tokio 非同步網路、共享 reqwest 連線池與 TCP_NODELAY。
- 商品狀態監測、開賣時間排程、喚醒延遲量測及取消。
- 共用節流器、Retry-After、有限次數退避重試與 timeout。
- 非約束報價的價格、幣別、有效期及使用者確認檢查。
- Axum 本機模擬伺服器，支援開賣時間、延遲、429、503 注入。
- DNS、TCP、TLS、HTTP 與 JSON 處理時間的 P50/P95/P99；保留原始數據。

## 下載與執行

- Windows x64：`shoppee-core-v0.1.0-windows-x86_64.zip`
- Linux x64：`shoppee-core-v0.1.0-linux-x86_64.zip`（glibc 2.35+）
- macOS Apple Silicon：`shoppee-core-v0.1.0-macos-arm64.zip`（未公證）
- `SHA256SUMS.txt`：下載包 SHA-256 校驗碼。

解壓縮後在 PowerShell／終端機執行，參閱包內 `QUICKSTART.txt`。這是命令列程式，沒有圖形介面。Windows 可能需要 Microsoft Visual C++ Runtime。bench/probe 需要 curl 7.70+；run/mock 不需要 curl。

## 驗證與範圍

核心版本的 GitHub CI 已通過 16 項 integration tests、格式、Clippy 及 release build；本次發布工作流程另外建置三種目標平台、在可原生執行的 runner 上執行 --version／--help，並在 Linux 重跑測試。

現有效能報告使用 700 筆本機請求；P99 存在明顯抖動，不能推論蝦皮尖峰延遲、搶購成功率或最佳 ISP／地區。原始數據與限制見原始碼的 reports/ANALYSIS.md。

尚未接入蝦皮正式 API，沒有下單／付款端點。流程停止在等待使用者確認；正式整合必須使用平台授權介面，遵守配額與安全限制。
