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

use std::collections::HashMap;

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
                cur = match trie[cur as usize].children.get(b) {
                    Some(n) => n,
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
    children: Children,
    final_: bool,
}

/// Compact trie children. Most trie nodes (deep suffix chains over unique
/// words) have 0 or 1 child — `None`/`One` keep those heap-allocation-free,
/// which is the bulk of the build-time memory win over a per-node B-tree map.
enum Children {
    None,
    One(u8, u32),
    Many(Vec<(u8, u32)>), // sorted by label
}

impl Default for Children {
    fn default() -> Self {
        Children::None
    }
}

impl Children {
    fn get(&self, b: u8) -> Option<u32> {
        match self {
            Children::None => None,
            Children::One(k, v) => (*k == b).then_some(*v),
            Children::Many(m) => m.binary_search_by_key(&b, |&(k, _)| k).ok().map(|i| m[i].1),
        }
    }

    fn insert(&mut self, b: u8, child: u32) {
        match self {
            Children::None => *self = Children::One(b, child),
            Children::One(k, v) => {
                if *k == b {
                    *v = child;
                } else {
                    let pair = (*k, *v);
                    let m = if *k < b {
                        vec![pair, (b, child)]
                    } else {
                        vec![(b, child), pair]
                    };
                    *self = Children::Many(m);
                }
            }
            Children::Many(m) => match m.binary_search_by_key(&b, |&(k, _)| k) {
                Ok(i) => m[i].1 = child,
                Err(i) => m.insert(i, (b, child)),
            },
        }
    }

    /// Sorted (label, child) pairs. Allocates — used once per node in the
    /// minimize pass, not during the memory-heavy trie build.
    fn collect_sorted(&self) -> Vec<(u8, u32)> {
        match self {
            Children::None => Vec::new(),
            Children::One(k, v) => vec![(*k, *v)],
            Children::Many(m) => m.clone(),
        }
    }
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
    let kids = trie[node as usize].children.collect_sorted();
    let mut trans = Vec::with_capacity(kids.len());
    for (label, child) in kids {
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

/// Unsigned LEB128.
pub(crate) fn write_uvarint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let mut byte = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if v == 0 {
            break;
        }
    }
}

/// Post-order over the DAG from `root`: every state appears after all its
/// targets, so a state's transition target offsets are already known when
/// it is written. Recursion depth is bounded by the longest key.
fn post_order(root: u32, canon: &[CanonState]) -> Vec<u32> {
    fn dfs(s: u32, canon: &[CanonState], visited: &mut [bool], order: &mut Vec<u32>) {
        if visited[s as usize] {
            return;
        }
        visited[s as usize] = true;
        for &(_, c) in &canon[s as usize].trans {
            dfs(c, canon, visited, order);
        }
        order.push(s);
    }
    let mut visited = vec![false; canon.len()];
    let mut order = Vec::with_capacity(canon.len());
    dfs(root, canon, &mut visited, &mut order);
    order
}

/// Format v3 — compact, byte-offset addressed (no offset table). Per state:
/// a flags byte (bit0 = final, bit1 = single-transition), then transitions.
///
/// - **Single-transition** node (bit1 set) → `[flags, label, delta]`. The
///   target's right-language count is *omitted*: with one outgoing edge the
///   ordinal walk never skips it, so its count is never read. These nodes are
///   the vast majority (unique-word suffix chains), so dropping `ntrans` + the
///   count here is the bulk of the size win.
/// - **Otherwise** (0 or ≥2 transitions) → `[flags, ntrans, (label, delta,
///   count)×ntrans]` (all LEB128).
///
/// Header: magic4 · ver1 · width1 · value_count u32 · root_off u32 ·
/// state_count u32 (= 18 bytes). States blob follows; values tail.
fn serialize(canon: &[CanonState], num: &[u64], root: u32, values: &[u64]) -> Vec<u8> {
    let width = value_width(values);
    let order = post_order(root, canon);

    let mut state_off = vec![u32::MAX; canon.len()];
    let mut blob: Vec<u8> = Vec::new();
    for &s in &order {
        let off = blob.len() as u32;
        state_off[s as usize] = off;
        let st = &canon[s as usize];
        if st.trans.len() == 1 {
            // single-transition fast form (no ntrans, no count)
            let (label, target) = st.trans[0];
            blob.push(u8::from(st.final_) | 0b10); // bit1 = single
            blob.push(label);
            write_uvarint(&mut blob, u64::from(off - state_off[target as usize]));
        } else {
            blob.push(u8::from(st.final_)); // bit1 = 0 → multi/leaf form
            write_uvarint(&mut blob, st.trans.len() as u64);
            for &(label, target) in &st.trans {
                // target written earlier (post-order) → offset known, < off.
                blob.push(label);
                write_uvarint(&mut blob, u64::from(off - state_off[target as usize]));
                write_uvarint(&mut blob, num[target as usize]);
            }
        }
    }
    let root_off = state_off[root as usize];

    let mut out: Vec<u8> = Vec::with_capacity(18 + blob.len() + values.len() * width as usize);
    out.extend_from_slice(b"IXFA");
    out.push(3); // version
    out.push(width);
    out.extend_from_slice(&(values.len() as u32).to_le_bytes());
    out.extend_from_slice(&root_off.to_le_bytes());
    out.extend_from_slice(&(canon.len() as u32).to_le_bytes());
    out.extend_from_slice(&blob);
    for &v in values {
        out.extend_from_slice(&v.to_le_bytes()[..width as usize]);
    }
    out
}
