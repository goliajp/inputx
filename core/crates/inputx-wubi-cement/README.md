# inputx-wubi-cement

Wubi-specific engine glue for Inputx — a ready-to-drive `WubiEngine`
state machine, auto-commit policies, simcode promote rules, and L0
(per-user pin) snapshot helpers.

Pairs with the [`inputx-wubi`](https://crates.io/crates/inputx-wubi)
facade (codec / decomp / dict / jianma / layer / stroke / zigen
primitives). Use the facade if you're writing your own Wubi IME and
want only the data + lookup primitives; use this cement if you want
the full stateful keystroke handler that Inputx itself uses.

## API

```rust
use inputx_wubi_cement::{AutoCommitPolicy, WubiEngine};

let mut engine = WubiEngine::new();
engine.set_policy(AutoCommitPolicy::OnFourCodesIfUnique);

for b in b"shan" {
    if let Some(committed) = engine.handle_letter(*b) {
        println!("committed: {committed}");
    }
}
let cands = engine.candidates();
println!("preedit: {} candidates: {:?}", engine.buffer_str(), cands);
```

## License

Dual-licensed under MIT OR Apache-2.0.
