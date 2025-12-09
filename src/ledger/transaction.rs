use crate::ledger::serialize_4dp_or_none;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TransactionType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

impl TransactionType {
    pub fn is_under_dispute(&self) -> bool {
        matches!(self, Self::Dispute)
    }
}

impl Display for TransactionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Deposit => write!(f, "deposit"),
            Self::Withdrawal => write!(f, "withdrawal"),
            Self::Dispute => write!(f, "dispute"),
            Self::Resolve => write!(f, "resolve"),
            Self::Chargeback => write!(f, "chargeback"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    #[serde(rename = "type")]
    pub _type: TransactionType,
    pub client: u16,
    pub tx: u32,
    #[serde(serialize_with = "serialize_4dp_or_none")]
    pub amount: Option<f64>,
}

/// Status of how an incoming transaction line was handled.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransactionStatus {
    Applied,
    IgnoredLocked,
    IgnoredMissingReference,
    FailedInsufficientFunds,
    FailedInvalidAmount,
    FailedDuplicateTxID,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub struct TransactionEntry {
    #[serde(flatten)]
    pub tx: Transaction,
    pub status: TransactionStatus,
}

impl Display for Transaction {
    // read account.rs for the exact same comment that I would write here
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

impl Transaction {
    pub fn new(_type: TransactionType, client: u16, tx: u32, amount: Option<f64>) -> Self {
        Self {
            _type,
            client,
            tx,
            amount,
        }
    }
}

pub fn transactions_to_csv<'a>(transactions: impl IntoIterator<Item = &'a Transaction>) -> String {
    let mut buf = vec!["type,client,tx,amount".to_string()];
    transactions.into_iter().for_each(|transaction| {
        buf.push(transaction.to_string());
    });
    buf.join("\n")
}

pub fn transaction_entries_to_csv<'a>(
    entries: impl IntoIterator<Item = &'a TransactionEntry>,
) -> String {
    let mut buf = vec!["type,client,tx,amount,status".to_string()];
    entries.into_iter().for_each(|entry| {
        let mut line = entry.tx.to_string();
        line.push_str(&format!(",{}", format_status(entry.status)));
        buf.push(line);
    });
    buf.join("\n")
}

fn format_status(status: TransactionStatus) -> &'static str {
    match status {
        TransactionStatus::Applied => "applied",
        TransactionStatus::IgnoredLocked => "ignored_locked",
        TransactionStatus::IgnoredMissingReference => "ignored_missing_reference",
        TransactionStatus::FailedInsufficientFunds => "failed_insufficient_funds",
        TransactionStatus::FailedInvalidAmount => "failed_invalid_amount",
        TransactionStatus::FailedDuplicateTxID => "failed_duplicate_tx_id",
    }
}
