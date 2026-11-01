use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShopeeProduct {
    pub shop_id: u64,
    pub item_id: u64,
    pub canonical_url: String,
}

/// Parse a Taiwan product identifier locally. No HTTP, redirects or private APIs.
pub fn parse(input: &str) -> Result<ShopeeProduct> {
    ensure!(input.len() <= 4096, "商品網址過長");
    let u = Url::parse(input.trim()).context("請貼上完整的 HTTPS 商品網址")?;
    ensure!(u.scheme() == "https", "只接受 HTTPS 商品網址");
    ensure!(
        u.username().is_empty() && u.password().is_none() && u.port().is_none(),
        "網址不可包含帳密或自訂連接埠"
    );
    if matches!(u.host_str(), Some("s.shopee.tw" | "shope.ee" | "tw.shp.ee")) {
        anyhow::bail!("這是短網址，請先在蝦皮開啟商品，再複製完整商品網址");
    }
    ensure!(
        matches!(u.host_str(), Some("shopee.tw" | "www.shopee.tw")),
        "目前僅支援台灣蝦皮 shopee.tw 商品網址"
    );
    let path = u.path().trim_end_matches('/');
    let (shop, item) = if let Some(rest) = path.strip_prefix("/product/") {
        let parts: Vec<_> = rest.split('/').collect();
        ensure!(parts.len() == 2, "商品網址格式不正確");
        (parts[0], parts[1])
    } else {
        let (_, ids) = path
            .rsplit_once("-i.")
            .context("找不到商品編號；請複製商品頁完整網址")?;
        ensure!(!ids.contains('/'), "商品網址格式不正確");
        ids.split_once('.').context("商品網址缺少商店或商品編號")?
    };
    let id = |s: &str| -> Result<u64> {
        ensure!(
            !s.is_empty() && s.bytes().all(|x| x.is_ascii_digit()),
            "商品編號必須為正整數"
        );
        let id: u64 = s.parse().context("商品編號超出範圍")?;
        ensure!(id > 0, "商品編號必須大於零");
        Ok(id)
    };
    let shop_id = id(shop)?;
    let item_id = id(item)?;
    Ok(ShopeeProduct {
        shop_id,
        item_id,
        canonical_url: format!("https://shopee.tw/product/{shop_id}/{item_id}"),
    })
}
