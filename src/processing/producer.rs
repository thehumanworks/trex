use std::path::PathBuf;
use tokio::sync::mpsc;

pub struct TransactionProducer {
    tx: mpsc::Sender<PathBuf>,
}

impl TransactionProducer {
    pub fn new(tx: mpsc::Sender<PathBuf>) -> Self {
        Self { tx }
    }

    pub async fn produce(&mut self, transaction_file: String) -> anyhow::Result<()> {
        self.tx.send(PathBuf::from(transaction_file)).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sends_path_through_channel() {
        let (tx, mut rx) = mpsc::channel(10);
        let mut producer = TransactionProducer::new(tx);

        producer.produce("test.csv".to_string()).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received, PathBuf::from("test.csv"));
    }

    #[tokio::test]
    async fn handles_multiple_files() {
        let (tx, mut rx) = mpsc::channel(10);
        let mut producer = TransactionProducer::new(tx);

        producer.produce("file1.csv".to_string()).await.unwrap();
        producer.produce("file2.csv".to_string()).await.unwrap();
        producer.produce("file3.csv".to_string()).await.unwrap();

        assert_eq!(rx.recv().await.unwrap(), PathBuf::from("file1.csv"));
        assert_eq!(rx.recv().await.unwrap(), PathBuf::from("file2.csv"));
        assert_eq!(rx.recv().await.unwrap(), PathBuf::from("file3.csv"));
    }

    #[tokio::test]
    async fn channel_closes_on_drop() {
        let (tx, mut rx) = mpsc::channel::<PathBuf>(10);
        let producer = TransactionProducer::new(tx);

        drop(producer);

        assert!(rx.recv().await.is_none());
    }
}
