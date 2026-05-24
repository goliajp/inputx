//! Build side: collect (key, value) pairs → minimal acyclic FSA + value
//! table → serialized bytes. Build-time only (not perf-critical), so it
//! favors a simple, verifiable two-phase construction:
//!
//!   1. Insert all keys into a plain trie.
//!   2. Minimize bottom-up by hash-consing equivalent sub-automata
//!      (Revuz's algorithm) — two states are equal iff same finality and
//!      same (label → canonical-target) transition set.
//!
//! Values are kept out of the automaton (it stays a pure key recognizer)
//! and emitted as a fixed-width array indexed by each key's sorted rank.

use std::collections::{BTreeMap, HashMap};

/// Accumulates (key, value) pairs and serializes a minimal FSA.
#[derive(Default)]
pub struct Builder {
    pairs: Vec<(Vec<u8>, u64)>,
}

impl Builder {
    pub fn new() -> Self {
        Self { pairs: Vec::new() }
    }

    /// Add a key→value entry. Duplicate keys: last insert wins. Keys may be
    /// inserted in any order (the builder sorts).
    pub fn insert(&mut self, key: &[u8], value: u64) {
        self.pairs.push((key.to_vec(), value));
    }

    /// Consume the builder and return the serialized FSA bytes.
    pub fn finish(mut self) -> Vec<u8> {
        // Sort by key; on duplicates keep the LAST inserted value.
        self.pairs.sort_by(|a, b| a.0.cmp(&b.0));
        self.pairs.dedup_by(|a, b| {
            if a.0 == b.0 {
                b.1 = a.1; // `a` is the later element; carry its value into the kept `b`
                true
            } else {
                false
            }
        });

        let values: Vec<u64> = self.pairs.iter().map(|(_, v)| *v).collect();

        // ── Phase 1: trie ──────────────────────────────────────────────
        let mut trie: Vec<TrieNode> = vec![TrieNode::default()]; // root = 0
        for (key, _) in &self.pairs {
            let mut cur = 0u32;
            for &b in key {
                cur = match trie[cur as usize].children.get(&b) {
                    Some(&n) => n,
                    None => {
                        let n = trie.len() as u32;
                        trie.push(TrieNode::default());
                        trie[cur as usize].children.insert(b, n);
                        n
                    }
                };
            }
            trie[cur as usize].final_ = true;
        }

        // ── Phase 2: minimize (hash-cons, post-order) ──────────────────
        let mut register: HashMap<StateKey, u32> = HashMap::new();
        let mut canon: Vec<CanonState> = Vec::new();
        let root = minimize(0, &trie, &mut register, &mut canon);

        // Right-language sizes (number of accepted strings from each state).
        let mut num = vec![None; canon.len()];
        for i in 0..canon.len() {
            compute_num(i as u32, &canon, &mut num);
        }
        let num: Vec<u64> = num.into_iter().map(|n| n.unwrap_or(0)).collect();

        serialize(&canon, &num, root, &values)
    }
}

#[derive(Default)]
struct TrieNode {
    children: BTreeMap<u8, u32>,
    final_: bool,
}

struct CanonState {
    final_: bool,
    trans: Vec<(u8, u32)>, // (label, canonical target id), sorted by label
}

type StateKey = (bool, Vec<(u8, u32)>);

/// Post-order hash-cons: returns the canonical id for `node`. Because the
/// input is a trie (a tree), each node is visited exactly once; the register
/// collapses structurally-identical sub-automata into shared states.
fn minimize(
    node: u32,
    trie: &[TrieNode],
    register: &mut HashMap<StateKey, u32>,
    canon: &mut Vec<CanonState>,
) -> u32 {
    let mut trans = Vec::with_capacity(trie[node as usize].children.len());
    for (&label, &child) in &trie[node as usize].children {
        let cid = minimize(child, trie, register, canon);
        trans.push((label, cid));
    }
    let final_ = trie[node as usize].final_;
    let key: StateKey = (final_, trans.clone());
    if let Some(&id) = register.get(&key) {
        return id;
    }
    let id = canon.len() as u32;
    canon.push(CanonState { final_, trans });
    register.insert(key, id);
    id
}

fn compute_num(id: u32, canon: &[CanonState], memo: &mut [Option<u64>]) -> u64 {
    if let Some(n) = memo[id as usize] {
        return n;
    }
    let st = &canon[id as usize];
    let mut n = u64::from(st.final_);
    for &(_, child) in &st.trans {
        n += compute_num(child, canon, memo);
    }
    memo[id as usize] = Some(n);
    n
}

fn value_width(values: &[u64]) -> u8 {
    let maxv = values.iter().copied().max().unwrap_or(0);
    if maxv <= 0xFF {
        1
    } else if maxv <= 0xFFFF {
        2
    } else if maxv <= 0xFFFF_FFFF {
        4
    } else {
        8
    }
}

fn serialize(canon: &[CanonState], num: &[u64], root: u32, values: &[u64]) -> Vec<u8> {
    let width = value_width(values);

    // States blob + per-state offsets (relative to blob start).
    let mut blob: Vec<u8> = Vec::new();
    let mut offsets: Vec<u32> = Vec::with_capacity(canon.len());
    for st in canon {
        offsets.push(blob.len() as u32);
        blob.push(u8::from(st.final_)); // bit0 = final
        blob.extend_from_slice(&(st.trans.len() as u16).to_le_bytes());
        for &(label, target) in &st.trans {
            blob.push(label);
            blob.extend_from_slice(&target.to_le_bytes());
            blob.extend_from_slice(&(num[target as usize] as u32).to_le_bytes());
        }
    }

    let mut out: Vec<u8> = Vec::with_capacity(18 + offsets.len() * 4 + blob.len() + values.len() * width as usize);
    out.extend_from_slice(b"IXFA");
    out.push(1); // version
    out.push(width);
    out.extend_from_slice(&(canon.len() as u32).to_le_bytes());
    out.extend_from_slice(&(values.len() as u32).to_le_bytes());
    out.extend_from_slice(&root.to_le_bytes());
    for off in &offsets {
        out.extend_from_slice(&off.to_le_bytes());
    }
    out.extend_from_slice(&blob);
    for &v in values {
        out.extend_from_slice(&v.to_le_bytes()[..width as usize]);
    }
    out
}
