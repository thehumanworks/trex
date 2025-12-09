use crate::ledger::{
    account::{Account, AccountId},
    transaction::{Transaction, TransactionEntry, TransactionStatus, TransactionType},
};
use log::warn;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Engine {
    accounts: HashMap<AccountId, Account>,
    // append-only immutable list of transactions (event source)
    transactions: Vec<TransactionEntry>,
    // transaction state (mutable - efficient retrieval of latest state)
    tx_state: HashMap<u32, TxState>,
}

impl Engine {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
            transactions: Vec::new(),
            tx_state: HashMap::new(),
        }
    }

    pub fn process(&mut self, tx: Transaction) {
        self.accounts
            .entry(tx.client)
            .or_insert_with(|| Account::new(tx.client));

        // negative state first, assume ignored due to chargeback lock
        // NOTE: this is used for logging, does not impact `account.is_locked()`
        let mut status = TransactionStatus::IgnoredLocked;

        let account = self.accounts.get_mut(&tx.client).unwrap();

        if account.is_locked() {
            self.transactions.push(TransactionEntry { tx, status });
            return;
        }

        let mut ensure_valid =
            |tx: Transaction, callable: &mut dyn FnMut() -> TransactionStatus| {
                if self.transactions.iter().any(|entry| entry.tx.tx.eq(&tx.tx)) {
                    status = TransactionStatus::FailedDuplicateTxID;
                } else if let Some(amount) = tx.amount
                    && &amount <= &0.0
                {
                    status = TransactionStatus::FailedInvalidAmount;
                } else {
                    status = callable();
                }
            };

        match tx._type {
            TransactionType::Deposit => {
                if let Some(amount) = tx.amount {
                    ensure_valid(tx.clone(), &mut || {
                        account.deposit(amount);
                        self.tx_state.insert(
                            tx.tx,
                            TxState {
                                client: tx.client,
                                amount,
                                dispute_state: DisputeState::Normal,
                            },
                        );
                        TransactionStatus::Applied
                    });
                } else {
                    status = TransactionStatus::FailedInvalidAmount;
                }
            }
            TransactionType::Withdrawal => {
                let Some(amount) = tx.amount else {
                    status = TransactionStatus::FailedInvalidAmount;
                    self.transactions.push(TransactionEntry { tx, status });
                    return;
                };

                ensure_valid(tx.clone(), &mut || match account.withdraw(amount) {
                    Ok(_) => {
                        self.tx_state.insert(
                            tx.tx,
                            TxState {
                                client: tx.client,
                                amount,
                                dispute_state: DisputeState::Normal,
                            },
                        );
                        TransactionStatus::Applied
                    }
                    Err(e) => {
                        warn!("Withdrawal error: {}", e);
                        TransactionStatus::FailedInsufficientFunds
                    }
                });
            }
            TransactionType::Dispute => {
                status = self
                    .tx_state
                    .get_mut(&tx.tx)
                    .and_then(|state| {
                        if state.client == tx.client && !state.is_under_dispute() {
                            account
                                .dispute(state.amount)
                                .map(|_| {
                                    state.dispute_state = DisputeState::Disputed;
                                    TransactionStatus::Applied
                                })
                                .map_err(|e| {
                                    warn!("Dispute error: {}", e);
                                })
                                .ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| {
                        warn!("Dispute error: no previous transaction found");
                        TransactionStatus::IgnoredMissingReference
                    });
            }
            TransactionType::Resolve => {
                status = self
                    .tx_state
                    .get_mut(&tx.tx)
                    .and_then(|state| {
                        if state.client == tx.client && state.is_under_dispute() {
                            account
                                .resolve(state.amount)
                                .map(|_| {
                                    state.dispute_state = DisputeState::Resolved;
                                    TransactionStatus::Applied
                                })
                                .map_err(|e| {
                                    warn!("Resolve error: {}", e);
                                })
                                .ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| {
                        warn!("Resolve error: no previous transaction in dispute state found");
                        TransactionStatus::IgnoredMissingReference
                    });
            }
            TransactionType::Chargeback => {
                status = self
                    .tx_state
                    .get_mut(&tx.tx)
                    .and_then(|state| {
                        if state.client == tx.client && state.is_under_dispute() {
                            account
                                .chargeback(state.amount)
                                .map(|_| {
                                    state.dispute_state = DisputeState::Chargeback;
                                    TransactionStatus::Applied
                                })
                                .map_err(|e| {
                                    warn!("Chargeback error: {}", e);
                                })
                                .ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| {
                        warn!("Chargeback error: no previous transaction in dispute state found");
                        TransactionStatus::IgnoredMissingReference
                    });
            }
        }

        // Append an event to the event source. Always.
        self.transactions.push(TransactionEntry { tx, status });
    }

    pub fn get_account(&self, account_id: AccountId) -> Option<&Account> {
        self.accounts.get(&account_id)
    }

    pub fn get_accounts(&self) -> &HashMap<AccountId, Account> {
        &self.accounts
    }

    pub fn get_transactions(&self) -> &Vec<TransactionEntry> {
        &self.transactions
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DisputeState {
    Normal,
    Disputed,
    Resolved,
    Chargeback,
}

#[derive(Debug, Clone, Copy)]
struct TxState {
    client: AccountId,
    amount: f64,
    dispute_state: DisputeState,
}

impl TxState {
    fn is_under_dispute(&self) -> bool {
        matches!(self.dispute_state, DisputeState::Disputed)
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use super::*;

    fn tx(t: TransactionType, client: u16, tx_id: u32, amount: Option<f64>) -> Transaction {
        Transaction::new(t, client, tx_id, amount)
    }

    #[test]
    fn chargeback_attempt_while_locked_is_rejected() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));
        engine.process(tx(TransactionType::Dispute, 1, 1, None));
        engine.process(tx(TransactionType::Chargeback, 1, 1, None));
        engine.process(tx(TransactionType::Chargeback, 1, 1, None));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 0.0);
        assert_eq!(account.held(), 0.0);
        assert_eq!(account.total(), 0.0);
        assert!(account.is_locked());
        assert!(
            engine
                .transactions
                .iter()
                .filter(|stored_tx| stored_tx.tx._type == TransactionType::Chargeback)
                .count()
                == 2
        );
        println!("{:?}", engine.transactions);
        assert!(
            engine
                .transactions
                .iter()
                .filter(
                    |stored_tx| stored_tx.tx._type == TransactionType::Chargeback
                        && stored_tx.status == TransactionStatus::IgnoredLocked
                )
                .count()
                == 1
        );
    }

    #[test]
    fn zero_amount_is_rejected() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(0.0)));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 0.0);
        assert_eq!(account.held(), 0.0);
        assert_eq!(account.total(), 0.0);
        assert!(!account.is_locked());
    }

    #[test]
    fn negative_amount_is_rejected() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(-100.0)));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 0.0);
        assert_eq!(account.held(), 0.0);
        assert_eq!(account.total(), 0.0);
        assert!(!account.is_locked());
    }

    #[test]
    fn duplicate_transactions_ids_are_rejected() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));
        engine.process(tx(TransactionType::Withdrawal, 1, 1, Some(50.0)));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 100.0);
        assert_eq!(account.held(), 0.0);
        assert_eq!(account.total(), 100.0);
        assert!(!account.is_locked());
    }

    #[test]
    fn deposit_credits_new_account() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 100.0);
        assert_eq!(account.held(), 0.0);
        assert_eq!(account.total(), 100.0);
        assert!(!account.is_locked());
    }

    #[test]
    fn deposit_credits_existing_account() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));
        assert!(engine.get_account(1).is_some());

        let account = engine.get_account(1).unwrap().clone();
        assert_eq!(account.total(), 100.0);
        assert_eq!(account.available(), 100.0);

        engine.process(tx(TransactionType::Deposit, 1, 2, Some(50.0)));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 150.0);
        assert_eq!(account.total(), 150.0);
    }

    #[test]
    fn withdrawal_creates_account_but_fails_with_zero_balance() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Withdrawal, 1, 1, Some(100.0)));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 0.0);
        assert_eq!(account.total(), 0.0);
    }

    #[test]
    fn withdrawal_insufficient_funds_does_not_update_balance() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));
        engine.process(tx(TransactionType::Withdrawal, 1, 2, Some(150.0)));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 100.0);
        assert_eq!(account.total(), 100.0);
    }

    #[test]
    fn withdrawal_debits_account() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));
        engine.process(tx(TransactionType::Withdrawal, 1, 2, Some(40.0)));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 60.0);
        assert_eq!(account.total(), 60.0);
    }

    #[test]
    fn withdrawal_insufficient_funds_fails_silently() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(50.0)));
        engine.process(tx(TransactionType::Withdrawal, 1, 2, Some(100.0)));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 50.0);
        assert_eq!(account.total(), 50.0);
    }

    #[test]
    fn dispute_moves_funds_to_held() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));
        engine.process(tx(TransactionType::Dispute, 1, 1, None));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 0.0);
        assert_eq!(account.held(), 100.0);
        assert_eq!(account.total(), 100.0);
        assert!(!account.is_locked());
    }

    #[test]
    fn resolve_releases_held_funds() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));
        engine.process(tx(TransactionType::Dispute, 1, 1, None));
        engine.process(tx(TransactionType::Resolve, 1, 1, None));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 100.0);
        assert_eq!(account.held(), 0.0);
        assert_eq!(account.total(), 100.0);
        assert!(!account.is_locked());
    }

    #[test]
    fn chargeback_removes_funds_and_locks() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));
        engine.process(tx(TransactionType::Dispute, 1, 1, None));
        engine.process(tx(TransactionType::Chargeback, 1, 1, None));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 0.0);
        assert_eq!(account.held(), 0.0);
        assert_eq!(account.total(), 0.0);
        assert!(account.is_locked());
    }

    #[test]
    fn locked_account_ignores_transactions() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));
        engine.process(tx(TransactionType::Dispute, 1, 1, None));
        engine.process(tx(TransactionType::Chargeback, 1, 1, None));
        // Account is now locked
        engine.process(tx(TransactionType::Deposit, 1, 2, Some(50.0)));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.total(), 0.0);
        assert!(account.is_locked());
    }

    #[test]
    fn cannot_dispute_nonexistent_tx() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));
        engine.process(tx(TransactionType::Dispute, 1, 999, None));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 100.0);
        assert_eq!(account.held(), 0.0);
    }

    #[test]
    fn cannot_dispute_another_clients_tx() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));
        engine.process(tx(TransactionType::Deposit, 2, 2, Some(50.0)));
        // Client 2 tries to dispute client 1's transaction
        engine.process(tx(TransactionType::Dispute, 2, 1, None));

        let account1 = engine.get_account(1).unwrap();
        let account2 = engine.get_account(2).unwrap();
        assert_eq!(account1.available(), 100.0);
        assert_eq!(account1.held(), 0.0);
        assert_eq!(account2.available(), 50.0);
        assert_eq!(account2.held(), 0.0);
    }

    #[test]
    fn cannot_dispute_already_disputed_tx() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));
        engine.process(tx(TransactionType::Deposit, 1, 2, Some(50.0)));
        engine.process(tx(TransactionType::Dispute, 1, 1, None));
        // Try to dispute again
        engine.process(tx(TransactionType::Dispute, 1, 1, None));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 50.0);
        assert_eq!(account.held(), 100.0);
    }

    #[test]
    fn cannot_resolve_non_disputed_tx() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));
        engine.process(tx(TransactionType::Resolve, 1, 1, None));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 100.0);
        assert_eq!(account.held(), 0.0);
    }

    #[test]
    fn cannot_chargeback_non_disputed_tx() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));
        engine.process(tx(TransactionType::Chargeback, 1, 1, None));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 100.0);
        assert_eq!(account.total(), 100.0);
        assert!(!account.is_locked());
    }

    #[test]
    fn cannot_resolve_already_resolved_tx() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));
        engine.process(tx(TransactionType::Dispute, 1, 1, None));
        engine.process(tx(TransactionType::Resolve, 1, 1, None));
        engine.process(tx(TransactionType::Resolve, 1, 1, None));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 100.0);
        assert_eq!(account.held(), 0.0);
    }

    #[test]
    fn can_redispute_after_resolve() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));
        engine.process(tx(TransactionType::Dispute, 1, 1, None));
        engine.process(tx(TransactionType::Resolve, 1, 1, None));
        engine.process(tx(TransactionType::Dispute, 1, 1, None));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 0.0);
        assert_eq!(account.held(), 100.0);
    }

    #[test]
    fn multiple_clients_independent() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));
        engine.process(tx(TransactionType::Deposit, 2, 2, Some(200.0)));
        engine.process(tx(TransactionType::Dispute, 1, 1, None));

        let account1 = engine.get_account(1).unwrap();
        let account2 = engine.get_account(2).unwrap();
        assert_eq!(account1.held(), 100.0);
        assert_eq!(account2.available(), 200.0);
        assert_eq!(account2.held(), 0.0);
    }

    #[test]
    fn partial_dispute_with_remaining_balance() {
        let mut engine = Engine::new();
        engine.process(tx(TransactionType::Deposit, 1, 1, Some(100.0)));
        engine.process(tx(TransactionType::Deposit, 1, 2, Some(50.0)));
        engine.process(tx(TransactionType::Dispute, 1, 1, None));

        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 50.0);
        assert_eq!(account.held(), 100.0);
        assert_eq!(account.total(), 150.0);
    }
}
