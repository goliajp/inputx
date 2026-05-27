//! Lazy range iteration — `Fsa::range(prefix)` returns an `Iterator`
//! that yields keys in lexicographic order. Compose with `.take()`,
//! `.filter()`, `.map()` etc. without materializing the full result.
//!
//! Run with: `cargo run --release --example range_iter -p inputx-fsa`

use inputx_fsa::{Builder, Fsa};

fn main() {
    let mut b = Builder::new();
    for (i, code) in [
        "apple", "apply", "april", "apt", "apex",
        "approach", "approve", "absolute", "absorb",
    ]
    .iter()
    .enumerate()
    {
        b.insert(code.as_bytes(), i as u64);
    }
    let fsa = Fsa::new(b.finish()).unwrap();

    println!("--- range(\"ap\") top 3 by lex order ---");
    for (key, value) in fsa.range(b"ap").take(3) {
        println!("  {:?} = {value}", std::str::from_utf8(&key).unwrap());
    }

    println!("\n--- whole-set iter() filtered to length >= 6 ---");
    let long: Vec<_> = fsa
        .iter()
        .filter(|(k, _)| k.len() >= 6)
        .map(|(k, v)| (String::from_utf8(k).unwrap(), v))
        .collect();
    for (k, v) in &long {
        println!("  {k:?} = {v}");
    }
    println!("({} entries with len >= 6)", long.len());
}
