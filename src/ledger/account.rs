use crate::ledger::serialize_4dp;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

pub type AccountId = u16;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Account {
    client: AccountId,
    #[serde(serialize_with = "serialize_4dp")]
    available: f64,
    #[serde(serialize_with = "serialize_4dp")]
    held: f64,
    #[serde(serialize_with = "serialize_4dp")]
    total: f64,
    locked: bool,
}

impl Display for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut buf = Vec::new();
        {
            let mut wtr = csv::WriterBuilder::new()
                .has_headers(false)
                .from_writer(&mut buf);
            wtr.serialize(self).map_err(|_| std::fmt::Error)?;
            wtr.flush().map_err(|_| std::fmt::Error)?;
        }
        let s = String::from_utf8(buf).map_err(|_| std::fmt::Error)?;
        write!(f, "{}", s.trim())
    }
}

impl Account {
    pub fn new(client: AccountId) -> Self {
        Self {
            client,
            available: 0.0,
            held: 0.0,
            total: 0.0,
            locked: false,
        }
    }

    pub fn deposit(&mut self, amount: f64) {
        self.available += amount;
        self.total += amount;
    }

    pub fn withdraw(&mut self, amount: f64) -> anyhow::Result<()> {
        if self.available < amount {
            anyhow::bail!("Insufficient available funds for withdrawal");
        }
        self.available -= amount;
        self.total -= amount;
        Ok(())
    }

    pub fn dispute(&mut self, amount: f64) -> anyhow::Result<()> {
        if self.available < amount {
            anyhow::bail!("Insufficient available funds for dispute");
        }
        self.available -= amount;
        self.held += amount;
        Ok(())
    }

    pub fn resolve(&mut self, amount: f64) -> anyhow::Result<()> {
        if self.held < amount {
            anyhow::bail!("Insufficient held funds");
        }
        self.held -= amount;
        self.available += amount;
        Ok(())
    }

    pub fn chargeback(&mut self, amount: f64) -> anyhow::Result<()> {
        if self.held < amount {
            anyhow::bail!("Insufficient held funds");
        }
        self.held -= amount;
        self.total -= amount;
        self.locked = true;
        Ok(())
    }

    pub fn client(&self) -> AccountId {
        self.client
    }

    pub fn available(&self) -> f64 {
        self.available
    }

    pub fn held(&self) -> f64 {
        self.held
    }

    pub fn total(&self) -> f64 {
        self.total
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }
}

pub fn accounts_to_csv<'a>(accounts: impl IntoIterator<Item = &'a Account>) -> String {
    let mut buf = vec!["client,available,held,total,locked".to_string()];
    accounts
        .into_iter()
        .for_each(|account| buf.push(account.to_string()));
    buf.join("\n")
}
