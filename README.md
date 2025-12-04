# TREX - The transaction engine

## Main Patterns Used

- Producer / Consumer pattern (uses multi producer / single consumer channel for transaction processing. Reduces backpressure on the processor in a real-world scenario.)
- Event Sourcing pattern (uses an append-only immutable list of transactions to store the event source. This is a very powerful pattern for financial systems because it allows for recoverability of state and the ability to replay events in case of a failure.)
- `Invalid state` as an acceptable value rather than an error (Errors as return values)

##  Entrypoint in `main.rs`

- Reads one/many CSV files, outputs to stdout - can be piped to a file
- Account statuses are printed at the end of the process (see REQUIREMENTS.md). Optionally, passing --log prints out the transaction log (immutable event source).
- All functionality is done in-process, no DBs, no additional infra, but the architecture is designed to be extensible.

## (Many) Notes on scale

- **Queuing**: the queuing mechanism would require a solution with strong durability and consistency guarantees. Something like Kafka, Redpanda, SQS, Pub/Sub... the most important in this case is ensuring transaction ordering.

- **Guaranteed Ordering**: the use of serial incrementing transaction IDs would not function in a distributed system. Either there would be clashes and frequent locks or there is a reliance on a single 'master writer' which becomes a bottleneck and defeats the purpose of the distributed system. At scale, ordering requires the generation of temporally aware IDs like UUIDv7, as well as systems for clock synchronization when leveraging multiple nodes.

- **Retries**: the current implementation simply rejects invalid transactions. A more robust solution would enforce out of process retries. This can be done either by following a simple Queue based solution, popping the most recent transaction, moving back to the back of the queue on failure; or by using solutions like dead letter queues and client-notifications. In a scenario where financial transactions are involved, idempotency and guarantees of delivery and reattempt are paramount.

- **Strong Consistency** (agreeing on state): the current implementation is not atomic. There are multiple patterns that can be followed here, a simple solution that is often very robust (when the database layer is compatible with distributed systems - e.g. CockroachDB) is optimistic concurrency. A simple requirement: each transaction must read the state of the system before attempting to transact. At the point of transaction, the entity sends the "expected_state" to the system and the system guardrails against unexpected changes by rejecting the transaction if the expected_state differs from the current state (i.e. a transaction acted after the one in-flight had a chance to write). This solution is very robust, since the "state" is a view/snapshot of the system (in event sourcing terms). This means that it can be read-only as it's purely the computation of state of the event source, which separates reads/writes - a very good practice in high throughput systems. In the solution implemented here, the state is a mutable hashmap, for simplicity, since this is a single process application and mutability of state is not a problem.

- **Recovery/Idempotency**: one of the most powerful features of an event sourcing system in applications that deal with financial transactions is the capability of recovering state. Since the transaction log (event source - see `src/ledger/engine.rs`) is an append-only list, you can reconstruct the state of an account at any point in time by simply iterating through the transactions. A system that does not store transaction logs will inevitably lose state and have angry customers shouting at the customer service team because they are certain they had 10k in their account a week ago and they demand a refund. And unfortunately, the `Mutable Financial Corp` that did not store the transaction log does not have a clue of whether that's true or not. Lesson: don't be like `Mutable Financial Corp`.

- **Fault Tolerance**: the current implementation is careful to handle invalid state. All invalid states are recorded in the log (see TxState's enum) and the choice to update an account's balance is always decided by the "Engine". The use of `mpsc` channel (mentioned before) is very useful here as the pressure on the engine can be configured and if the engine is "too busy", the channel holds. There are scenarios where I am forcing an application crash: if the input data does not conform to the spec the application exits gracefully (the last line of main - `consumer.consume().await`, which returns the parsing error as a `anyhow::Error`).

## Key Takeaways

In a single process application, transaction ordering, enforcing strong consistency are not problematic or over-ambitious endeavours.

> The use of channels to control backpressure is something I believe is good practice to prevent rate-limiting errors or application crashes downstream, so I'm using the pattern as it makes it clear that there is a need to control throughput - in a Rust multi-threaded application I can say from experience that under reasonable memory and compute resources an application could likely manage without a message passing mechanism and rely purely on a strong distributed database. It does not negate that the pattern is good practice.

### Footnote

All things considered, the most important in a financial system would be: guarantees of transaction validity, recoverability of state (rollbacks, snapshotting...), fault tolerance (failures don't lead to unexpected crashes, unless the crash is intentional to protect the system from malicious activity) and "perceived performance" (meaning that the API does not wait for the transaction to process - it gets "accepted" and the processing of the transaction is done in the background, at the pace it needs to be done to ensure valid transactions).
