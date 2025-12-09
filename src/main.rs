use std::env;
use tokio::sync::mpsc;
use trex::{
    ledger::{account::accounts_to_csv, engine::Engine, transaction::transaction_entries_to_csv},
    processing::{consumer::TransactionConsumer, producer::TransactionProducer},
};

async fn run_engine(input: &str, mode: ProcessingMode) -> anyhow::Result<Engine> {
    let (tx, rx) = mpsc::channel(100);
    let consumer = TransactionConsumer::new(rx, Engine::new());
    let mut producer = TransactionProducer::new(tx);

    match mode {
        ProcessingMode::SingleFile => {
            let path = input;
            producer.produce(path.to_string()).await?;
        }
        ProcessingMode::MultiFile => {
            let paths = input.split(',').collect::<Vec<&str>>();
            for path in paths {
                producer.produce(path.to_string()).await?;
            }
        }
    }
    drop(producer);
    consumer.consume().await
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ProcessingMode {
    #[default]
    SingleFile,
    MultiFile,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 || args.len() > 3 {
        eprintln!(
            "Usage: {} <transactions.csv[,file2.csv,...]> [--log]",
            args[0]
        );
        std::process::exit(1);
    }

    let print_log = args.get(2).map(|s| s == "--log").unwrap_or(false);

    let processing_mode = if args[1].contains(",") {
        ProcessingMode::MultiFile
    } else {
        ProcessingMode::default()
    };

    let engine = run_engine(&args[1], processing_mode).await?;
    if print_log {
        println!(
            "{}",
            transaction_entries_to_csv(engine.get_transactions().iter())
        );
    } else {
        println!("{}", accounts_to_csv(engine.get_accounts().values()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 0.0001,
            "expected {expected}, got {actual}"
        );
    }

    #[tokio::test]
    async fn whitespace_is_handled_correctly() {
        let engine = run_engine("data/input/whitespace.csv", ProcessingMode::SingleFile)
            .await
            .expect("engine should process whitespace.csv");
        let accounts = engine.get_accounts();
        assert_eq!(accounts.len(), 2);
    }

    #[tokio::test]
    async fn full_flow_dataset_matches_expected_balances() {
        let engine = run_engine("data/input/full_flow_large.csv", ProcessingMode::SingleFile)
            .await
            .expect("engine should process full_flow_large.csv");
        let accounts = engine.get_accounts();
        assert_eq!(accounts.len(), 4);

        let c1 = accounts.get(&1).unwrap();
        assert_close(c1.available(), 0.0);
        assert_close(c1.held(), 0.0);
        assert_close(c1.total(), 0.0);
        assert!(!c1.is_locked());

        let c2 = accounts.get(&2).unwrap();
        assert_close(c2.available(), 0.0);
        assert_close(c2.held(), 0.0);
        assert_close(c2.total(), 0.0);
        assert!(c2.is_locked());

        let c3 = accounts.get(&3).unwrap();
        assert_close(c3.available(), 80.0);
        assert_close(c3.held(), 0.0);
        assert_close(c3.total(), 80.0);
        assert!(!c3.is_locked());

        let c4 = accounts.get(&4).unwrap();
        assert_close(c4.available(), 0.0001);
        assert_close(c4.held(), 0.0);
        assert_close(c4.total(), 0.0001);
        assert!(!c4.is_locked());
    }

    #[tokio::test]
    async fn spec_violations_are_ignored_and_locking_is_respected() {
        let engine = run_engine("data/input/spec_violations.csv", ProcessingMode::SingleFile)
            .await
            .expect("engine should process spec_violations.csv");
        let accounts = engine.get_accounts();
        assert_eq!(accounts.len(), 2);

        let c1 = accounts.get(&1).unwrap();
        assert_close(c1.available(), 7.0);
        assert_close(c1.held(), 0.0);
        assert_close(c1.total(), 7.0);
        assert!(!c1.is_locked());

        let c2 = accounts.get(&2).unwrap();
        assert_close(c2.available(), 0.0);
        assert_close(c2.held(), 0.0);
        assert_close(c2.total(), 0.0);
        assert!(c2.is_locked());
    }
}
