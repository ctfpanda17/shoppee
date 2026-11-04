# Shoppee Local Companion v0.3.0

This unpacked Chrome / Edge extension runs with your existing browser login.
It reads rendered product controls and can select options/quantity and click
「直接購買」 once. It never clicks the final order/payment button.

1. Unzip the release archive to a permanent folder.
2. Open chrome://extensions or edge://extensions, enable Developer mode,
   choose Load unpacked, and select the companion-extension folder.
3. Start shoppee-core.exe. In its local UI choose 瀏覽器選規格, click 連接瀏覽器,
   paste a full product URL and click 讀取真實規格.
4. In the opened Shopee tab, complete any login/verification yourself, then
   return to the local UI and read again if necessary.
5. Choose each option, quantity and merchandise subtotal cap. Set the time,
   then start the task. Keep the local UI tab open until it reports completion.
6. Review shipping, discounts and the final total in Shopee before placing an order.

The displayed cap covers merchandise only; the final shipping-inclusive total
is checked by you at checkout. An unsupported address, disabled option, ambiguous
price, changed DOM, expired command or login/security screen stops the task.
No automated retry of a purchase click, no private APIs, no cookies/password
extraction and no CAPTCHA handling. Permissions cover Shopee product DOM and
loopback UI pages. No session credentials are sent to the Rust app or anywhere else.

Only public product data is returned to the local UI. Do not load this extension
into an untrusted modified UI. This is a website-specific adapter, not an official
Shopee SDK; DOM changes can require an update. Inspecting/selection were verified
on the supplied FlashFire listing. End-to-end order placement is not implemented.
