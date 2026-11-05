use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Product {
    pub id: String,
    pub available: bool,
    pub price_minor: u64,
    pub currency: String,
}

/// A non-binding quote, never an order or payment instruction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingConfirmation {
    pub product: Product,
    pub expires_unix_ms: u64,
    pub requires_user_confirmation: bool,
}

impl PendingConfirmation {
    pub fn validate(&self, id: &str, max_price_minor: u64, currency: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.requires_user_confirmation,
            "adapter must require user confirmation"
        );
        anyhow::ensure!(
            self.product.id == id && self.product.available,
            "invalid quote product"
        );
        anyhow::ensure!(
            self.product.price_minor <= max_price_minor,
            "price exceeds approved limit"
        );
        anyhow::ensure!(self.product.currency == currency, "currency mismatch");
        anyhow::ensure!(
            self.expires_unix_ms > crate::schedule::unix_ms()?,
            "quote expired"
        );
        Ok(())
    }
}
