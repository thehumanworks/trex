# Review

1. Fails to deal with whitespace in input CSV:

Comments:

- the csv crate does not trim whitespace by default. Trimming needs to be added via trim(csv::Trim::All).
- fix in @src/processing/consumer.rs:19-21

#### Test run before fix

```bash
> cargo test whitespace_is_handled_correctly

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 29 filtered out; finished in 0.00s

     Running unittests src/main.rs (target/debug/deps/trex-de984dac7543a16a)

running 1 test
test tests::whitespace_is_handled_correctly ... FAILED

failures:

---- tests::whitespace_is_handled_correctly stdout ----

thread 'tests::whitespace_is_handled_correctly' (1274903) panicked at src/main.rs:82:14:
engine should process whitespace.csv: CSV deserialize error: record 1 (line: 2, byte: 26): missing field `type`
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    tests::whitespace_is_handled_correctly

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s

error: test failed, to rerun pass `--bin trex`
(base)
```

#### Test run after fix

```bash
   Compiling trex v0.1.0 (/Users/mish/Work/trex)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.63s
     Running unittests src/lib.rs (target/debug/deps/trex-2c5557df53099218)

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 29 filtered out; finished in 0.00s

     Running unittests src/main.rs (target/debug/deps/trex-de984dac7543a16a)

running 1 test
test tests::whitespace_is_handled_correctly ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s
```

---

2. Duplicate transaction IDs are not handled:

Comments:

- `tx_state.insert()` silently overwrites existing entries when a duplicate `tx_id` is processed
- This corrupts dispute/chargeback lookups since the original transaction data is lost
- Per spec (line 54): "transaction IDs (tx) are globally unique" - duplicates should be rejected (I misread as a 'guarantee' of uniqueness in input)
- fix in `src/ledger/engine.rs:47-54` (deposit) and `src/ledger/engine.rs:69-76` (withdrawal)

**Fix**
- Fixed by adding a closure called `ensure_valid` in `engine.rs`.
- Test added `duplicate_transactions_ids_are_rejected`.

---

3. Negative amounts are not validated:

Comments:

- No validation that `amount > 0` for deposits or withdrawals
- A deposit with negative amount subtracts from balance (bypasses withdrawal checks)
- A withdrawal with negative amount adds to balance (free money exploit)
- fix in `src/ledger/engine.rs:43-57` (deposit) and `src/ledger/engine.rs:59-83` (withdrawal)

**Fix**
- Updated `ensure_valid` closure to check for amounts equal or lower to 0.0;
- Tests added `zero_amount_is_rejected` and `negative_amount_is_rejected`

---

4. Chargebacks can be applied to already-chargebacked transactions:

This is not true. The function starts with the negative assumption of a locked account state.
- if the account is `locked`, the status is marked as `TransactionStatus::IgnoreLocked` and it does not process further.
