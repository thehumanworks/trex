## TREX Challenge

Implement a toy payments engine that reads transactions from CSV, updates client accounts, handles disputes and chargebacks, and outputs account states as CSV.

```
cargo run -- transactions.csv > accounts.csv
```

## Criteria

| Category | Description |
|----------|-------------|
| Basics | Does it build? Does it read/write data correctly? |
| Completeness | Handle all cases: deposits, withdrawals, disputes, resolutions, chargebacks |
| Correctness | Are cases handled correctly? Include test data and unit tests. Document in README |
| Safety | How are errors handled? Justify dangerous choices |
| Efficiency | Stream data, don't load everything upfront. Consider concurrent TCP streams |
| Maintainability | Clean code > efficient code. Humans will review without explanation |

## Input

CSV with columns: `type` (string), `client` (u16), `tx` (u32), `amount` (decimal, 4 decimal places)

```csv
type, client, tx, amount
deposit, 1, 1, 1.0
deposit, 2, 2, 2.0
withdrawal, 1, 3, 1.5
```

- Client IDs unique per client, not ordered
- Transaction IDs globally unique, not ordered
- Transactions are chronologically ordered in file
- Accept whitespace and up to 4 decimal places

## Output

CSV with columns: `client`, `available`, `held`, `total`, `locked`

```csv
client,available,held,total,locked
1,1.5,0,1.5,false
2,2,0,2,false
```

| Column | Description |
|--------|-------------|
| available | Funds available for use (total - held) |
| held | Funds held for dispute (total - available) |
| total | All funds (available + held) |
| locked | Account frozen (true if chargeback occurred) |

Row ordering and decimal formatting don't matter. Precision: 4 decimal places.

## Transaction Types

### Deposit

Credits client account. Increases `available` and `total`.

### Withdrawal

Debits client account. Decreases `available` and `total`. Fails silently if insufficient funds.

### Dispute

References a transaction by `tx`. No `amount` field. Moves funds from `available` to `held`. `total` unchanged. Ignore if `tx` doesn't exist.

### Resolve

Releases disputed funds. Moves from `held` back to `available`. `total` unchanged. Ignore if `tx` doesn't exist or isn't disputed.

### Chargeback

Final dispute state. Decreases `held` and `total` by disputed amount. Freezes account (`locked = true`). Ignore if `tx` doesn't exist or isn't disputed.

## Assumptions

- Single asset account per client
- Create new client record if doesn't exist
- Clients are u16, transactions are u32
- When in doubt, make bank-like assumptions and document them
