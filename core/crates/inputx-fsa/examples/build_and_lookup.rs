//! Build an `Fsa` from a few (key, value) pairs, serialize it, then
//! load + lookup. Mirrors the typical "build once at offline time,
//! `include_bytes!` into the binary, query forever" workflow.
//!
//! Run with: `cargo run --release --example build_and_lookup -p inputx-fsa`

use inputx_fsa::{Builder, Fsa};

fn main() {
    let mut b = Builder::new();
    for (k, v) in [
        (&b"apple"[..], 1u64),
        (b"apply", 2),
        (b"apricot", 3),
        (b"banana", 4),
        (b"band", 5),
    ] {
        b.insert(k, v);
    }
    let bytes = b.finish();
    println!("serialized {} bytes", bytes.len());

    let fsa = Fsa::new(bytes).expect("valid fsa");
    println!("entries: {}", fsa.len());

    for key in [&b"apple"[..], b"apply", b"absent"] {
        match fsa.get(key) {
            Some(v) => println!("get({:?}) = {v}", std::str::from_utf8(key).unwrap()),
            None => println!("get({:?}) = None", std::str::from_utf8(key).unwrap()),
        }
    }
}
