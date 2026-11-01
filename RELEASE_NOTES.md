# shoppee-core v0.2.0 — 本機商品任務 UI

## 新增功能

- 不帶參數啟動程式即可開啟瀏覽器 UI，單一 Rust 執行檔內嵌所有介面資源。
- 貼上台灣蝦皮完整商品網址並解析商店／商品編號，移除追蹤參數。
- 「蝦皮商品頁」模式：立即或定時開啟官方商品頁，由使用者在蝦皮完成購買。
- 「本機模擬」模式：自動管理 mock，監測開賣、檢查預算、取得報價並等待模擬確認。
- 本地時間排程、倒數、停止任務、狀態、HTTP P50/P95/P99、紀錄與 JSON 匯出。
- 本機工作階段驗證、Host／Origin 限制、CSP、請求大小及設定範圍檢查。
- Windows 包含 START_UI.cmd 與 HELP.cmd；舊有命令列工具保留。

## 使用

Windows 解壓縮後雙擊 `shoppee-core.exe` 或 `START_UI.cmd`。macOS／Linux 執行 `./shoppee-core ui`。保留終端機視窗；關閉視窗或 Ctrl+C 可結束服務。

支援網址 `https://shopee.tw/product/123/456` 與 `https://shopee.tw/商品名稱-i.123.456`。短網址請先開啟商品後複製完整網址。啟動時間使用電腦本地時區，設定只保留在執行中的程式內。

## 明確限制

**未接入可供本專案使用的蝦皮買家下單授權介面，因此沒有「貼網址就自動下單」能力，也沒有真實付款端點。** 蝦皮模式不會讀取庫存／價格、選擇規格、加入購物車或送出訂單；到點只開啟官方商品頁。UI 不要求帳密或 Cookie。正式 API 整合仍需平台核准的買家交易介面與對應授權文件。

模擬報價固定 NT$99.00，與真實商品無關；模擬確認不會建立真實交易。倒數依本機時鐘與 OS 排程，不提供硬即時保證。取消只適用於待派發工作，已開啟的瀏覽器頁面不會被撤回。

三個下載包為 Windows x64、Linux x64（glibc 2.35+）、macOS Apple Silicon；macOS 版本未公證。SHA256SUMS.txt 提供下載包校驗碼。UI 不需要 Node.js／Python；bench/probe 另需 curl 7.70+。

原始碼附核心與 UI integration tests。reports/ANALYSIS.md 與既有 JSON 為 v0.1.0 的歷史基準，不能當作 UI 或真實蝦皮的量測結果。
