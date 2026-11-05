# Shoppee Local Companion v0.3.5

Chrome / Edge extension using your own existing browser login. Automation stops
at checkout; final order, shipping-inclusive total and payment are confirmed by you.

1. Close the older app. Unzip the full release. At chrome://extensions or
   edge://extensions disable/remove older Companion copies, then Load unpacked
   from the new companion-extension folder. Keep only one copy active.
2. Refresh Shopee product/cart tabs and the local UI. Start START_UI.cmd or
   shoppee-core ui; open the UI in the same browser and click 連接瀏覽器.
3. Paste the product URL, read variants, choose every SKU, quantity, cart-total
   budget cap and time. Keep the app and local UI open.
4. The product page selects SKU/quantity and goes to the cart. Unit/original
   prices are informational. The cart checks identity, complete SKU, quantity,
   stock/selection and the visible bottom 總金額 next to 去買單.
5. A stable total at or below the cap permits one Go Checkout click. Missing,
   ambiguous, malformed or over-budget totals stop. Zero is allowed when explicit.
6. Review shipping/fees, store pickup, final payable amount and order in Shopee.

The budget gate uses the cart footer total, not unit-price times quantity or
raw monetary values in product rows. Cart discounts are already reflected in
that displayed total; do not subtract vouchers again. Later checkout shipping
or fees can change the final amount; the program does not place/pay the order.

An authenticated receipt contains only cart total (minor units), selected-item
count and task quantity, plus public product data. Rust checks the receipt and
exports it under job.browser_cart. Old extensions without the receipt fail
closed. A direct /checkout URL or a closed message channel cannot invent a total.

No cart selections are changed. Other selected goods, unknown scopes, duplicate
SKU rows, dialogs, wrong quantity, login/security screens and expiry stop. Header
checkboxes are accepted only with identified scope. Clicks are not retried.
No private APIs, CAPTCHA handling, rate-limit evasion, password/cookie reading or
address extraction. Tests use synthetic DOM and mocked navigation; real account
end-to-end checkout is not yet verified. DOM changes may require updates.
