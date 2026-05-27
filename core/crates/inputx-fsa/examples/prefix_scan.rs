//! Prefix scan — visit every key starting with a given prefix without
//! allocating a `Vec` of results. This is the hot path the Inputx IME
//! uses for partial-input candidate generation.
//!
//! Run with: `cargo run --release --example prefix_scan -p inputx-fsa`

use inputx_fsa::{Builder, Fsa};

fn main() {
    let mut b = Builder::new();
    for (i, word) in [
        "apple", "apply", "apricot", "approve", "approach",
        "banana", "band", "bandage", "candy", "card",
    ]
    .iter()
    .enumerate()
    {
        b.insert(word.as_bytes(), i as u64);
    }
    let fsa = Fsa::new(b.finish()).unwrap();

    println!("--- contains_prefix probes ---");
    for p in ["app", "ban", "ca", "zz"] {
        println!("{p:?} → {}", fsa.contains_prefix(p.as_bytes()));
    }

    println!("\n--- prefix_for_each(\"app\") ---");
    let mut count = 0;
    fsa.prefix_for_each(b"app", |key, value| {
        count += 1;
        println!("  {:?} = {value}", std::str::from_utf8(key).unwrap());
    });
    println!("(visited {count})");

    println!("\n--- prefix(\"ban\") — eager Vec form ---");
    for (key, value) in fsa.prefix(b"ban") {
        println!("  {:?} = {value}", std::str::from_utf8(&key).unwrap());
    }
}
