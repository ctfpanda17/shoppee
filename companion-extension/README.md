# Shoppee Local Companion v0.3.4

Updating from v0.3.0: reload the extension from the new release folder, refresh
both Shopee and local UI tabs, then reconnect. The adapter now allows up to 12
seconds for purchase data to stabilize (still 1.2 seconds stable), diagnoses
missing price, selection and quantity separately, and ignores title animations.

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


v0.3.2 handles an intermediate /cart page automatically. It requires exactly
one selected checkbox and one unambiguous target product row with the exact
SKU, quantity, unit price and subtotal, plus a matching one-item cart total.
It waits for stable data and clicks only 去買單 once, then verifies /checkout.
Other selected items, unknown checkbox scopes, duplicate target rows,
modals and ambiguous data stop the task. It does not change cart selections.
Tests use synthetic cart DOM based on the reported screenshot, not a captured
live DOM. Real account checkout remains unverified. No final order is submitted.


v0.3.3 reads the rendered Stardust/ARIA checkbox state before a backing native
input. A checked framework wrapper can legitimately have an unchecked backing
input. Conflicting rendered/ARIA states remain blocked. Shop/cart header controls
are accepted as aggregates only when their containing product rows can be
identified and their scope has no unaccounted product links; checked aggregates
must agree with those item states. Other selected goods, unknown controls and
the one-item footer total remain guarded. Reload both tabs after updating.


v0.3.4 prefers standalone rendered money elements in the target item row,
excluding hidden and crossed-out originals. Inline discount/shipping labels do
not become extra unit-price cells. Ambiguous extra monetary elements still stop
the task. Price failures include the compared product-page price, observed cart
candidates, quantity, cap and extraction source in the UI log/export. A failed
job's browser_catalog may still be the initial inspection catalog; its null
price is not evidence of a changed checkout price. Strict price equality,
quantity, subtotal and budget checks remain enabled.

When loading a new release folder, disable/remove older Companion copies first.
Keep one extension active, then refresh both Shopee and local UI tabs.
