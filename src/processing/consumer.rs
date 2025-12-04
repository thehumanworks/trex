use std::path::PathBuf;

use csv::Reader;
use tokio::sync::mpsc;

use crate::ledger::{engine::Engine, transaction::Transaction};

pub struct TransactionConsumer {
    rx: mpsc::Receiver<PathBuf>,
    engine: Engine,
}

impl TransactionConsumer {
    pub fn new(rx: mpsc::Receiver<PathBuf>, engine: Engine) -> Self {
        Self { rx, engine }
    }

    pub async fn consume(mut self) -> anyhow::Result<Engine> {
        while let Some(path) = self.rx.recv().await {
            let mut reader = Reader::from_path(path)?;

            for result in reader.deserialize::<Transaction>() {
                let tx: Transaction = result?;
                self.engine.process(tx);
            }
        }
        Ok(self.engine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_csv(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    #[tokio::test]
    async fn parses_and_processes_valid_csv() {
        let csv = create_csv("type,client,tx,amount\ndeposit,1,1,100.0\nwithdrawal,1,2,50.0\n");
        let (path_tx, path_rx) = mpsc::channel(10);

        let consumer = TransactionConsumer::new(path_rx, Engine::new());

        path_tx.send(csv.path().to_path_buf()).await.unwrap();
        drop(path_tx);

        let engine = consumer.consume().await.unwrap();
        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 50.0);
        assert_eq!(account.total(), 50.0);
    }

    #[tokio::test]
    async fn handles_empty_csv() {
        let csv = create_csv("type,client,tx,amount\n");
        let (path_tx, path_rx) = mpsc::channel(10);

        let consumer = TransactionConsumer::new(path_rx, Engine::new());

        path_tx.send(csv.path().to_path_buf()).await.unwrap();
        drop(path_tx);

        let engine = consumer.consume().await.unwrap();
        assert!(engine.get_accounts().is_empty());
    }

    #[tokio::test]
    async fn processes_multiple_files_in_sequence() {
        let csv1 = create_csv("type,client,tx,amount\ndeposit,1,1,10.0\n");
        let csv2 = create_csv("type,client,tx,amount\ndeposit,2,2,20.0\ndeposit,2,3,30.0\n");

        let (path_tx, path_rx) = mpsc::channel(10);

        let consumer = TransactionConsumer::new(path_rx, Engine::new());

        path_tx.send(csv1.path().to_path_buf()).await.unwrap();
        path_tx.send(csv2.path().to_path_buf()).await.unwrap();
        drop(path_tx);

        let engine = consumer.consume().await.unwrap();
        assert_eq!(engine.get_account(1).unwrap().available(), 10.0);
        assert_eq!(engine.get_account(2).unwrap().available(), 50.0);
    }

    #[tokio::test]
    async fn handles_transactions_without_amount() {
        let csv =
            create_csv("type,client,tx,amount\ndeposit,1,1,100.0\ndispute,1,1,\nresolve,1,1,\n");
        let (path_tx, path_rx) = mpsc::channel(10);

        let consumer = TransactionConsumer::new(path_rx, Engine::new());

        path_tx.send(csv.path().to_path_buf()).await.unwrap();
        drop(path_tx);

        let engine = consumer.consume().await.unwrap();
        let account = engine.get_account(1).unwrap();
        assert_eq!(account.available(), 100.0);
        assert_eq!(account.held(), 0.0);
    }

    #[tokio::test]
    async fn exits_when_producer_closes() {
        let (path_tx, path_rx) = mpsc::channel::<PathBuf>(10);

        let consumer = TransactionConsumer::new(path_rx, Engine::new());

        drop(path_tx);

        let result = consumer.consume().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn preserves_chronological_order() {
        let csv = create_csv(
            "type,client,tx,amount\n\
             deposit,1,101,100.0\n\
             dispute,1,101,\n\
             chargeback,1,101,\n",
        );
        let (path_tx, path_rx) = mpsc::channel(10);

        let consumer = TransactionConsumer::new(path_rx, Engine::new());

        path_tx.send(csv.path().to_path_buf()).await.unwrap();
        drop(path_tx);

        let engine = consumer.consume().await.unwrap();
        let account = engine.get_account(1).unwrap();
        assert_eq!(account.total(), 0.0);
        assert!(account.is_locked());
    }

    #[tokio::test]
    async fn full_dispute_flow() {
        let csv = create_csv(
            "type,client,tx,amount\n\
             deposit,1,1,100.0\n\
             deposit,2,2,200.0\n\
             withdrawal,1,3,50.0\n\
             dispute,1,1,\n\
             resolve,1,1,\n",
        );
        let (path_tx, path_rx) = mpsc::channel(10);

        let consumer = TransactionConsumer::new(path_rx, Engine::new());

        path_tx.send(csv.path().to_path_buf()).await.unwrap();
        drop(path_tx);

        let engine = consumer.consume().await.unwrap();

        let acc1 = engine.get_account(1).unwrap();
        assert_eq!(acc1.available(), 50.0);
        assert_eq!(acc1.held(), 0.0);

        let acc2 = engine.get_account(2).unwrap();
        assert_eq!(acc2.available(), 200.0);
    }
}
