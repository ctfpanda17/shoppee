//! A local UI-to-extension command broker. Never handles website credentials.
use crate::{product_url, schedule};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tokio::sync::{oneshot, Mutex, Notify};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionItem {
    pub label: String,
    pub disabled: bool,
    pub selected: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub name: String,
    pub options: Vec<OptionItem>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub canonical_url: String,
    pub title: String,
    pub groups: Vec<Group>,
    pub unit_price_minor: Option<u64>,
    pub stock: Option<u32>,
    pub shipping_blocked: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub group: String,
    pub option: String,
}
impl Catalog {
    pub fn validate(&self, url: &str) -> Result<()> {
        ensure!(
            self.canonical_url == url && product_url::parse(url)?.canonical_url == url,
            "商品識別不符"
        );
        ensure!(
            !self.title.is_empty() && self.title.len() <= 1000,
            "商品標題無效"
        );
        ensure!(self.groups.len() <= 8, "規格群組過多");
        for (i, g) in self.groups.iter().enumerate() {
            ensure!(
                g.options.iter().filter(|o| o.selected).count() <= 1,
                "同一規格群組出現多個選取項，停止操作"
            );
            ensure!(
                !g.name.is_empty()
                    && g.name.len() <= 200
                    && !g.options.is_empty()
                    && g.options.len() <= 100,
                "規格資料無效"
            );
            ensure!(
                !self.groups[..i].iter().any(|other| other.name == g.name),
                "規格群組名稱重複"
            );
            for (n, o) in g.options.iter().enumerate() {
                ensure!(!o.label.is_empty() && o.label.len() <= 500, "規格名稱無效");
                ensure!(
                    !g.options[..n].iter().any(|other| other.label == o.label),
                    "規格選項名稱重複"
                );
            }
        }
        ensure!(
            self.unit_price_minor
                .is_none_or(|p| p > 0 && p <= 100_000_000_000),
            "價格資料無效"
        );
        Ok(())
    }
    pub fn validate_selection(&self, selections: &[Selection]) -> Result<()> {
        ensure!(selections.len() == self.groups.len(), "請選擇所有規格");
        for group in &self.groups {
            let choices: Vec<_> = selections
                .iter()
                .filter(|s| s.group == group.name)
                .collect();
            ensure!(choices.len() == 1, "規格群組必須各選一項");
            ensure!(
                group
                    .options
                    .iter()
                    .any(|o| o.label == choices[0].option && !o.disabled),
                "所選規格不存在或目前停用"
            );
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Inspect,
    Checkout,
}
#[derive(Debug, Clone, Serialize)]
pub struct Command {
    pub id: u64,
    pub kind: Kind,
    pub url: String,
    pub selections: Vec<Selection>,
    pub quantity: u32,
    pub max_price_minor: u64,
    pub expires_unix_ms: u64,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub catalog: Catalog,
    pub checkout_opened: bool,
    pub payment_sent: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub command_id: u64,
    pub outcome: Option<Outcome>,
    pub error: Option<String>,
}
struct Pending {
    command: Command,
    claimed: bool,
    tx: oneshot::Sender<Result<Outcome>>,
}
#[derive(Default)]
struct Inner {
    next_id: u64,
    pending: Option<Pending>,
}
#[derive(Clone, Default)]
pub struct Broker {
    inner: Arc<Mutex<Inner>>,
    notify: Arc<Notify>,
}
impl Broker {
    pub async fn dispatch(
        &self,
        kind: Kind,
        url: String,
        selections: Vec<Selection>,
        quantity: u32,
        max_price_minor: u64,
    ) -> Result<Outcome> {
        let (tx, rx) = oneshot::channel();
        let id = {
            let mut inner = self.inner.lock().await;
            ensure!(inner.pending.is_none(), "已有瀏覽器操作等待完成");
            inner.next_id += 1;
            let id = inner.next_id;
            inner.pending = Some(Pending {
                command: Command {
                    id,
                    kind,
                    url,
                    selections,
                    quantity,
                    max_price_minor,
                    expires_unix_ms: schedule::unix_ms()? + 60_000,
                },
                claimed: false,
                tx,
            });
            id
        };
        self.notify.notify_one();
        let result = tokio::time::timeout(Duration::from_secs(60), rx).await;
        let mut inner = self.inner.lock().await;
        if inner.pending.as_ref().is_some_and(|p| p.command.id == id) {
            inner.pending.take();
        }
        drop(inner);
        result
            .context("瀏覽器操作逾時，請確認擴充功能、登入狀態與操作頁仍開啟")?
            .context("瀏覽器操作已取消")?
    }
    pub async fn claim(&self) -> Option<Command> {
        loop {
            let notified = self.notify.notified();
            {
                let mut inner = self.inner.lock().await;
                if let Some(p) = inner.pending.as_mut() {
                    if !p.claimed {
                        p.claimed = true;
                        return Some(p.command.clone());
                    }
                }
            }
            if tokio::time::timeout(Duration::from_secs(20), notified)
                .await
                .is_err()
            {
                return None;
            }
        }
    }
    pub async fn report(&self, report: Report) -> Result<()> {
        let mut inner = self.inner.lock().await;
        let p = inner.pending.as_ref().context("此操作已取消或逾時")?;
        ensure!(
            p.command.id == report.command_id && p.claimed,
            "瀏覽器操作識別不符"
        );
        ensure!(
            schedule::unix_ms()? <= p.command.expires_unix_ms,
            "操作已逾時"
        );
        ensure!(
            report.outcome.is_some() != report.error.is_some(),
            "回報格式不正確"
        );
        let result = if let Some(outcome) = report.outcome {
            outcome.catalog.validate(&p.command.url)?;
            ensure!(!outcome.payment_sent, "禁止由橋接器送出訂單或付款");
            match p.command.kind {
                Kind::Inspect => ensure!(!outcome.checkout_opened, "讀取規格不可進入結帳"),
                Kind::Checkout => {
                    ensure!(outcome.checkout_opened, "尚未進入結帳");
                    ensure!(!outcome.catalog.shipping_blocked, "目前配送方式不可用");
                    outcome.catalog.validate_selection(&p.command.selections)?;
                    ensure!(
                        p.command.selections.iter().all(|s| outcome
                            .catalog
                            .groups
                            .iter()
                            .any(|g| g.name == s.group
                                && g.options.iter().any(|o| o.label == s.option && o.selected))),
                        "實際規格未與設定一致"
                    );
                    let total = outcome
                        .catalog
                        .unit_price_minor
                        .context("無法驗證價格")?
                        .checked_mul(u64::from(p.command.quantity))
                        .context("商品小計超出範圍")?;
                    ensure!(total <= p.command.max_price_minor, "商品小計超過預算");
                    ensure!(
                        outcome
                            .catalog
                            .stock
                            .is_none_or(|s| s >= p.command.quantity),
                        "庫存不足"
                    );
                }
            }
            Ok(outcome)
        } else {
            let error = report.error.unwrap();
            ensure!(!error.is_empty() && error.len() <= 500, "錯誤訊息過長");
            Err(anyhow::anyhow!(error))
        };
        let pending = inner.pending.take().unwrap();
        let _ = pending.tx.send(result);
        Ok(())
    }
    pub async fn cancel(&self) {
        self.inner.lock().await.pending.take();
        self.notify.notify_waiters();
    }
}
