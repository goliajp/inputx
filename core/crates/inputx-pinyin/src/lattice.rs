//! Phase-3 candidate lattice for cross-path joint scoring.
//!
//! # Why a lattice
//!
//! The Phase 0/1/2 architecture ran each candidate-generation path
//! (Path 1a Exact / 1b Fuzzy / 1c Typo / 2 Abbrev / 0b Viterbi / Path 5)
//! independently and unioned the results. That makes it impossible to
//! ask "given the bigram LM, which combination of cross-path choices
//! produces the best whole-sentence reading?". Phase 3 (CP-3.1..3.6)
//! migrates each path into edges of a single lattice, then runs one
//! beam-Viterbi over the lattice — the LM scores transitions *between*
//! edges, including edges from different paths.
//!
//! # Shape
//!
//! For a buffer of N bytes the lattice has N+1 nodes (positions 0..=N).
//! Each [`Edge`] connects two positions and says "buffer\[from..to\]
//! resolves to this candidate word, with this log-prob from the dict
//! (= log of the prior × likelihood) and this channel kind". The
//! channel kind adds its own log-prob penalty:
//!
//! | kind            | penalty                       | rationale                                |
//! |-----------------|-------------------------------|------------------------------------------|
//! | `Exact`         | 0                             | user typed the real reading              |
//! | `Fuzzy(p)`      | p = log10 P(fuzzy \| true)    | e.g. z/zh swap, ~ -2 .. -3               |
//! | `Typo(p)`       | p = log10 P(typo \| true)     | 2-consonant typo rescue, ~ -2.5 default  |
//! | `Abbrev(p)`     | p = log10 P(abbrev \| full)   | simplified-pinyin "zg→中国", ~ -3.5      |
//!
//! # Beam Viterbi
//!
//! [`Graph::viterbi`] runs a left-to-right beam DP that, at each node,
//! keeps the top-`beam` partial paths reaching that node. Each
//! transition takes the previous path's score, the new edge's
//! [`Edge::weight`] (dict log-prob + channel penalty), and adds a
//! caller-supplied LM term `log10 P(curr | prev)` over the last word
//! of the previous path. This is the spot where the Phase-2 KenLM
//! plugs in.
//!
//! # Status
//!
//! CP-3.1 ships this module **uncalled**. CP-3.2..3.6 migrate the
//! historical paths in one at a time behind a `USE_LATTICE` const
//! gate; rollback is "flip the const back".
//!
//! # Example
//!
//! ```
//! use inputx_pinyin::lattice::{Edge, EdgeKind, Graph};
//!
//! // Buffer "xianjin" — 7 bytes, 8 positions, but we use a 3-node
//! // toy here just to illustrate the API.
//! let mut g = Graph::for_buffer(7);
//! // "xian" = bytes 0..4
//! g.add_edge(Edge::exact(0, 4, "现", -2.0));
//! g.add_edge(Edge::exact(0, 4, "先", -2.5));
//! g.add_edge(Edge::exact(0, 4, "鲜", -3.0));
//! // "jin" = bytes 4..7
//! g.add_edge(Edge::exact(4, 7, "金", -2.0));
//! g.add_edge(Edge::exact(4, 7, "进", -2.5));
//! g.add_edge(Edge::exact(4, 7, "京", -3.0));
//!
//! // Without LM: top-1 is 现金 (-2.0 + -2.0 = -4.0).
//! let paths = g.viterbi(3, |_, _| 0.0);
//! assert_eq!(paths[0].sentence(), "现金");
//!
//! // With a mock LM that boosts "现金" and "先进":
//! let paths = g.viterbi(3, |prev, curr| match (prev, curr) {
//!     ("现", "金") => 1.0,
//!     ("先", "进") => 1.0,
//!     _ => 0.0,
//! });
//! let top_two: Vec<String> = paths.iter().take(2).map(|p| p.sentence()).collect();
//! assert!(top_two.contains(&"现金".to_string()));
//! assert!(top_two.contains(&"先进".to_string()));
//! ```

use core::cmp::Ordering;

use crate::dict::PinyinDict;

/// Phase-3 path migration trait. Each historical candidate-generation
/// path (Path 1a Exact, Path 1b Fuzzy, Path 1c Typo, Path 2 Abbrev, ...)
/// implements this trait to populate a shared lattice with its edges.
/// CP-3.2..3.5 add one impl at a time; CP-3.6 wires the result back
/// into the candidate stream behind the `PINYIN_USE_LATTICE` env gate.
///
/// Implementors are stateless — they take the buffer, the dict (for
/// lookup), and a mutable `Graph` to add edges into. Returning `usize`
/// = number of edges added is for diagnostics; production code ignores
/// it.
pub trait PathToLattice {
    /// Populate `graph` with all edges this path would emit for `buffer`.
    /// Returns the number of edges added.
    fn populate_lattice(&self, buffer: &str, dict: &PinyinDict, graph: &mut Graph) -> usize;
}

/// Phase-3 CP-3.3 channel log-prob table for the 9 fuzzy-pair swaps.
///
/// Each entry is `(from_substring, to_substring, log10 P(swap))`
/// where the user typed `to_substring` but the real pinyin is
/// `from_substring`. Values follow the climb-plan estimate (-2 to -3,
/// "13-15% substitution probability"); the order of magnitude
/// roughly matches the per-pair substitution rates we see in soak
/// reports — z/zh and in/ing are the noisiest at ~10% (≈ log10 -1.0)
/// while r/l and f/h are rarer at ~1% (≈ log10 -2.0). The actual
/// per-pair values are tuned in CP-3.6's λ sweep; CP-3.3 just plugs
/// them into the lattice as channel costs so the legacy
/// `FUZZY_DISCOUNT * 0.3` flat penalty is no longer hard-coded into
/// the score path.
pub const FUZZY_CHANNEL_LOG_PROBS: &[(&str, &str, f32)] = &[
    // Initial-position swaps (6 pairs, 12 directions).
    ("zh", "z",  -1.0),  ("z",  "zh", -1.0),
    ("ch", "c",  -1.0),  ("c",  "ch", -1.0),
    ("sh", "s",  -1.0),  ("s",  "sh", -1.0),
    ("n",  "l",  -1.5),  ("l",  "n",  -1.5),
    ("f",  "h",  -2.0),  ("h",  "f",  -2.0),
    ("r",  "l",  -2.0),  ("l",  "r",  -2.0),
    // Final-position swaps (3 pairs, 6 directions).
    ("ing", "in",  -1.0), ("in",  "ing", -1.0),
    ("eng", "en",  -1.0), ("en",  "eng", -1.0),
    ("ang", "an",  -1.0), ("an",  "ang", -1.0),
];

/// Compute the channel cost of substituting `variant` for the user's
/// typed syllable `typed`. Walks [`FUZZY_CHANNEL_LOG_PROBS`] for the
/// single applicable swap; returns `0.0` when they're identical
/// (canonical, i.e. EdgeKind::Exact territory).
pub fn fuzzy_channel_log_prob(typed: &str, variant: &str) -> f32 {
    if typed == variant {
        return 0.0;
    }
    // Initial-position: typed = prefix + rest, variant = swap + rest.
    for (from, to, p) in FUZZY_CHANNEL_LOG_PROBS {
        if let (Some(rest_typed), Some(rest_variant)) =
            (typed.strip_prefix(to), variant.strip_prefix(from))
            && rest_typed == rest_variant
        {
            return *p;
        }
        if let (Some(head_typed), Some(head_variant)) =
            (typed.strip_suffix(to), variant.strip_suffix(from))
            && head_typed == head_variant
        {
            return *p;
        }
    }
    // Should not happen if `variant` came from FuzzyConfig::expand —
    // but if the caller passes an unrelated pair, treat it as
    // maximally penalised so the Viterbi doesn't accidentally pick it.
    -10.0
}

/// Phase-3 CP-3.2: Path 1a (Exact pinyin lookup) implementation.
///
/// For a single-syllable buffer (the simple case the migration starts
/// with), looks every dict entry up at `(0, buf.len)` and emits an
/// [`EdgeKind::Exact`] edge per candidate. Edge log_prob is the
/// natural log of the dict's score (PHRASE_BASE-shifted freq, see
/// [`PinyinDict::lookup_with_scores_into`]).
///
/// Multi-syllable buffers currently fall through with zero edges —
/// segmentation lives in CP-3.6 once enough paths have migrated to
/// make it worth wiring the segmenter into the lattice.
pub struct Path1aExact;

/// Phase-3 CP-3.3: Path 1b (Fuzzy pinyin lookup) implementation.
///
/// For a single-syllable buffer, calls [`crate::fuzzy::FuzzyConfig::expand`]
/// to enumerate the 1-2 fuzzy variants the user might have typed, looks
/// each variant up in the dict, and emits one [`EdgeKind::Fuzzy`] edge
/// per candidate. The channel log-prob comes from
/// [`fuzzy_channel_log_prob`] — replacing the legacy
/// `FUZZY_DISCOUNT * 0.3` flat penalty with a per-pair empirical value.
///
/// Like [`Path1aExact`], the canonical (zero-swap) syllable is skipped
/// — Path 1a covers that.
pub struct Path1bFuzzy {
    /// Which fuzzy pairs are enabled; respects user preferences.
    pub fuzzy: crate::fuzzy::FuzzyConfig,
}

/// Phase-3 CP-3.4: Path 1c (2-consonant typo rescue) implementation.
///
/// Unlike Path 1a / 1b which can resolve the user's typo entirely from
/// the dict the lattice already holds, Path 1c needs the cross-crate
/// `INITIALS_INDEX` that lives in inputx-core (it maps a 2-consonant
/// cluster like "py" to the full-pinyin candidates "pin yin"). To keep
/// the lattice module free of the inputx-core dependency, Path1cTypo
/// is not a [`PathToLattice`] impl; instead [`Path1cTypo::add_edges`]
/// is a thin helper the production caller (CP-3.6 wiring) invokes
/// after it has resolved the typo cluster on its own — Path1cTypo just
/// turns the `(word, score)` resolutions into [`EdgeKind::Typo`]
/// edges with a uniform channel cost.
///
/// `channel_log_prob` defaults to `log10(0.05) ≈ -1.301` — the
/// climb-plan estimate of 5% P(2-consonant typo | true pinyin). CP-3.6
/// will sweep this alongside λ.
pub struct Path1cTypo {
    /// log10 P(typo | true). Default = -1.301 = log10(0.05).
    pub channel_log_prob: f32,
}

impl Default for Path1cTypo {
    fn default() -> Self {
        Self {
            channel_log_prob: Self::DEFAULT_CHANNEL_LOG_PROB,
        }
    }
}

impl Path1cTypo {
    /// log10(0.05) ≈ -1.301; the climb-plan default P(typo|true) = 5%.
    pub const DEFAULT_CHANNEL_LOG_PROB: f32 = -1.301029995663981;

    /// Turn pre-resolved typo candidates `(word, score)` into
    /// [`EdgeKind::Typo`] edges spanning the whole buffer. Returns the
    /// number of edges added.
    pub fn add_edges(
        &self,
        buffer: &str,
        resolutions: &[(String, f64)],
        graph: &mut Graph,
    ) -> usize {
        let n = buffer.len();
        if n == 0 || resolutions.is_empty() {
            return 0;
        }
        for (word, score) in resolutions {
            let log_prob = (score.max(1.0) as f32).log10();
            graph.add_edge(Edge::typo(0, n, word.clone(), log_prob, self.channel_log_prob));
        }
        resolutions.len()
    }
}

/// Phase-3 CP-3.5: Path 2 (简拼 abbreviation) implementation.
///
/// Like Path 1c, Path 2 needs an external resolution — the user typed
/// initials only (e.g. "zg" → 中国) and the production code resolves
/// the cluster against the cross-crate `INITIALS_INDEX`. Path2Abbrev
/// just translates `(word, score)` pairs into [`EdgeKind::Abbrev`]
/// edges with a uniform channel cost.
///
/// `channel_log_prob` defaults to `log10(0.02) ≈ -1.699` — the
/// climb-plan estimate of 2% P(initials abbreviation | full pinyin).
/// Abbrev is "more deliberate" than a typo so its base probability
/// is lower than Path 1c's 5%.
pub struct Path2Abbrev {
    /// log10 P(abbrev | full). Default = log10(0.02) ≈ -1.699.
    pub channel_log_prob: f32,
}

impl Default for Path2Abbrev {
    fn default() -> Self {
        Self {
            channel_log_prob: Self::DEFAULT_CHANNEL_LOG_PROB,
        }
    }
}

impl Path2Abbrev {
    /// log10(0.02) ≈ -1.699; the climb-plan default P(abbrev|full) = 2%.
    pub const DEFAULT_CHANNEL_LOG_PROB: f32 = -1.6989700043360187;

    /// Turn pre-resolved abbrev candidates `(word, score)` into
    /// [`EdgeKind::Abbrev`] edges spanning the whole buffer.
    pub fn add_edges(
        &self,
        buffer: &str,
        resolutions: &[(String, f64)],
        graph: &mut Graph,
    ) -> usize {
        let n = buffer.len();
        if n == 0 || resolutions.is_empty() {
            return 0;
        }
        for (word, score) in resolutions {
            let log_prob = (score.max(1.0) as f32).log10();
            graph.add_edge(Edge::abbrev(0, n, word.clone(), log_prob, self.channel_log_prob));
        }
        resolutions.len()
    }
}

impl PathToLattice for Path1bFuzzy {
    fn populate_lattice(&self, buffer: &str, dict: &PinyinDict, graph: &mut Graph) -> usize {
        let n = buffer.len();
        if n == 0 {
            return 0;
        }
        let mut added = 0;
        let variants = self.fuzzy.expand(buffer);
        // variants[0] is the canonical syllable; skip it — that's Path 1a's job.
        for variant in variants.iter().skip(1) {
            let channel = fuzzy_channel_log_prob(buffer, variant);
            let mut scratch: Vec<(String, f64)> = Vec::new();
            dict.lookup_with_scores_into(variant, &mut scratch);
            for (word, score) in scratch {
                let log_prob = (score.max(1.0) as f32).log10();
                graph.add_edge(Edge::fuzzy(0, n, word, log_prob, channel));
                added += 1;
            }
        }
        added
    }
}

impl PathToLattice for Path1aExact {
    fn populate_lattice(&self, buffer: &str, dict: &PinyinDict, graph: &mut Graph) -> usize {
        let n = buffer.len();
        if n == 0 {
            return 0;
        }
        let mut scratch: Vec<(String, f64)> = Vec::new();
        dict.lookup_with_scores_into(buffer, &mut scratch);
        let mut added = 0;
        for (word, score) in scratch {
            // Dict score is positive (PHRASE_BASE + freq, possibly L0-
            // boosted). Convert to log10 (negative when below 1.0 but
            // in practice always well above 1.0 thanks to PHRASE_BASE).
            let log_prob = (score.max(1.0) as f32).log10();
            graph.add_edge(Edge::exact(0, n, word, log_prob));
            added += 1;
        }
        added
    }
}

/// One word (or phrase) chosen for a single edge.
pub type Word = String;

/// Channel-specific log-prob adjustment, log10 scale, negative
/// (Exact == 0). Carries its own penalty so [`Edge::weight`] doesn't
/// need to know which channel produced it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EdgeKind {
    /// User typed pinyin matches the candidate's real reading.
    Exact,
    /// Fuzzy variant. f32 = log10 P(fuzzy | true), typically -2..-3.
    Fuzzy(f32),
    /// 2-consonant typo rescue. f32 = log10 P(typo | true), default ~-2.5.
    Typo(f32),
    /// Simplified-pinyin abbreviation. f32 = log10 P(abbrev | full), default ~-3.5.
    Abbrev(f32),
}

impl EdgeKind {
    /// Channel-specific penalty (≤ 0).
    pub fn channel_log_prob(&self) -> f32 {
        match self {
            EdgeKind::Exact => 0.0,
            EdgeKind::Fuzzy(p) | EdgeKind::Typo(p) | EdgeKind::Abbrev(p) => *p,
        }
    }
}

/// A single edge: "buffer bytes \[from..to\] resolve to `candidate`,
/// with this dict log-prob and this channel kind".
#[derive(Debug, Clone)]
pub struct Edge {
    /// Source node (buffer byte index, 0-based).
    pub from: usize,
    /// Destination node (buffer byte index past the last consumed byte).
    pub to: usize,
    /// The candidate word (or phrase) chosen for this span.
    pub candidate: Word,
    /// log10(prior × likelihood) from the dict (typically negative).
    pub log_prob: f32,
    /// Which channel produced this edge — controls the additional penalty.
    pub kind: EdgeKind,
}

impl Edge {
    /// Construct an exact-pinyin edge (no channel penalty).
    pub fn exact(from: usize, to: usize, candidate: impl Into<Word>, log_prob: f32) -> Self {
        Self { from, to, candidate: candidate.into(), log_prob, kind: EdgeKind::Exact }
    }

    /// Construct a fuzzy-channel edge. `channel_log_prob` is log10 P(fuzzy | true).
    pub fn fuzzy(
        from: usize, to: usize, candidate: impl Into<Word>, log_prob: f32, channel_log_prob: f32,
    ) -> Self {
        Self {
            from, to, candidate: candidate.into(), log_prob,
            kind: EdgeKind::Fuzzy(channel_log_prob),
        }
    }

    /// Construct a typo-rescue edge. `channel_log_prob` is log10 P(typo | true).
    pub fn typo(
        from: usize, to: usize, candidate: impl Into<Word>, log_prob: f32, channel_log_prob: f32,
    ) -> Self {
        Self {
            from, to, candidate: candidate.into(), log_prob,
            kind: EdgeKind::Typo(channel_log_prob),
        }
    }

    /// Construct a simplified-pinyin (abbrev) edge. `channel_log_prob` is log10 P(abbrev | full).
    pub fn abbrev(
        from: usize, to: usize, candidate: impl Into<Word>, log_prob: f32, channel_log_prob: f32,
    ) -> Self {
        Self {
            from, to, candidate: candidate.into(), log_prob,
            kind: EdgeKind::Abbrev(channel_log_prob),
        }
    }

    /// Total edge weight = dict log-prob + channel penalty.
    pub fn weight(&self) -> f32 {
        self.log_prob + self.kind.channel_log_prob()
    }
}

/// A reconstructed Viterbi path through the lattice.
#[derive(Debug, Clone)]
pub struct Path {
    /// The sequence of words picked along this path, in buffer order.
    pub words: Vec<Word>,
    /// The matching sequence of edges, parallel to `words`.
    pub edges: Vec<Edge>,
    /// Total log-prob = Σ edge.weight() + Σ LM bigram log P(curr|prev).
    pub score: f32,
}

impl Path {
    /// Concatenate the words into a single sentence.
    pub fn sentence(&self) -> String {
        let mut s = String::with_capacity(self.words.iter().map(|w| w.len()).sum());
        for w in &self.words {
            s.push_str(w);
        }
        s
    }
}

/// The lattice itself. A buffer of N bytes uses N+1 positions
/// (0..=N); edges connect any pair `from < to` where the substring
/// `buf[from..to]` has at least one candidate.
#[derive(Debug, Clone, Default)]
pub struct Graph {
    n_positions: usize,
    edges: Vec<Edge>,
}

impl Graph {
    /// Empty graph for a buffer of `n_bytes` bytes (so `n_bytes + 1`
    /// positions).
    pub fn for_buffer(n_bytes: usize) -> Self {
        Self { n_positions: n_bytes + 1, edges: Vec::new() }
    }

    /// Number of positions (= buffer length + 1).
    pub fn n_positions(&self) -> usize {
        self.n_positions
    }

    /// All edges in insertion order.
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// Insert an edge. Panics on out-of-range or empty-span (`from >= to`).
    pub fn add_edge(&mut self, edge: Edge) {
        assert!(
            edge.from < self.n_positions,
            "edge.from {} out of range (n_positions {})",
            edge.from, self.n_positions
        );
        assert!(
            edge.to < self.n_positions,
            "edge.to {} out of range (n_positions {})",
            edge.to, self.n_positions
        );
        assert!(edge.from < edge.to, "edge has empty span {}..{}", edge.from, edge.to);
        self.edges.push(edge);
    }

    /// Run beam-Viterbi top-K decoding.
    ///
    /// `beam` bounds the number of partial paths kept at each node, *and*
    /// is also the number of final paths returned. `lm_log_prob` is
    /// queried at every edge transition for `log10 P(curr_word | prev_word)`;
    /// for the very first edge of a path it is called with `prev = ""`,
    /// which most LM impls will treat as "no context" / 0.
    ///
    /// Paths that don't cover the whole buffer (i.e. don't reach the
    /// last node) are simply not returned. If no path covers the whole
    /// buffer, the result is empty.
    pub fn viterbi<F>(&self, beam: usize, mut lm_log_prob: F) -> Vec<Path>
    where
        F: FnMut(&str, &str) -> f32,
    {
        if beam == 0 || self.n_positions == 0 {
            return Vec::new();
        }

        // Group edges by their `from` position so we can iterate
        // outgoing edges per node without rescanning the whole edge list.
        let mut edges_from: Vec<Vec<&Edge>> = vec![Vec::new(); self.n_positions];
        for e in &self.edges {
            edges_from[e.from].push(e);
        }

        // Partial path stored cheaply as a back-pointer chain in `parents`:
        // parents[i] = (prev_idx_in_node, edge_idx_in_self.edges).
        // dp[i] = Vec<(score, last_word_index_in_self.edges)>
        //
        // Simpler-but-OK alternative used here: store the full Path
        // clone per partial. With beam ≤ ~16 and n_positions ≤ ~30
        // this is fine; if it ever shows up in a profile, swap for
        // back-pointers.
        type Partial = (f32, Vec<Word>, Vec<Edge>);

        let mut dp: Vec<Vec<Partial>> = vec![Vec::new(); self.n_positions];
        dp[0].push((0.0, Vec::new(), Vec::new()));

        for i in 0..self.n_positions - 1 {
            let here = core::mem::take(&mut dp[i]);
            for (prev_score, prev_words, prev_edges) in &here {
                let prev_last = prev_words.last().map(String::as_str).unwrap_or("");
                for e in &edges_from[i] {
                    let lm = lm_log_prob(prev_last, &e.candidate);
                    let new_score = prev_score + e.weight() + lm;
                    let mut new_words = prev_words.clone();
                    new_words.push(e.candidate.clone());
                    let mut new_edges = prev_edges.clone();
                    new_edges.push((*e).clone());
                    dp[e.to].push((new_score, new_words, new_edges));
                }
            }
            // Prune dp[to] cells as they get populated.
            // Cheap approach: leave pruning until after the whole node
            // is processed. We do that for *all later nodes* once per
            // outer-loop iteration. Since each entry in dp[k] gets
            // visited exactly once in the outer loop (when i = k),
            // pruning at the boundary "after we've stopped adding to k"
            // is equivalent to pruning at the start of iter k. We do it
            // lazily inside the iteration for nodes already populated.
            for k in (i + 1)..self.n_positions {
                if dp[k].len() > beam {
                    dp[k].sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));
                    dp[k].truncate(beam);
                }
            }
        }

        // Final beam: top-`beam` partials reaching the last node.
        let mut final_partials = core::mem::take(&mut dp[self.n_positions - 1]);
        final_partials.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal));
        final_partials.truncate(beam);
        final_partials
            .into_iter()
            .map(|(score, words, edges)| Path { score, words, edges })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn noop_lm(_: &str, _: &str) -> f32 {
        0.0
    }

    #[test]
    fn edge_kind_penalty_signs() {
        assert_eq!(EdgeKind::Exact.channel_log_prob(), 0.0);
        assert_eq!(EdgeKind::Fuzzy(-2.5).channel_log_prob(), -2.5);
        assert_eq!(EdgeKind::Typo(-3.0).channel_log_prob(), -3.0);
        assert_eq!(EdgeKind::Abbrev(-3.5).channel_log_prob(), -3.5);
    }

    #[test]
    fn edge_weight_sums_dict_and_channel() {
        let e = Edge::fuzzy(0, 2, "你好", -1.2, -2.5);
        assert!((e.weight() - (-3.7)).abs() < 1e-6);
    }

    /// Toy: 2 nodes / 1 edge — Viterbi finds the only path.
    #[test]
    fn single_edge_lattice() {
        let mut g = Graph::for_buffer(4);
        g.add_edge(Edge::exact(0, 4, "你好", -1.0));
        let paths = g.viterbi(3, noop_lm);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].sentence(), "你好");
        assert!((paths[0].score - (-1.0)).abs() < 1e-6);
    }

    /// Toy: ambiguity at one position. Viterbi top-1 picks the higher
    /// log-prob word.
    #[test]
    fn single_position_ambiguity() {
        let mut g = Graph::for_buffer(4);
        // "你好" at -1.0 vs "拟好" at -3.0
        g.add_edge(Edge::exact(0, 4, "你好", -1.0));
        g.add_edge(Edge::exact(0, 4, "拟好", -3.0));
        let paths = g.viterbi(3, noop_lm);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].sentence(), "你好");
        assert_eq!(paths[1].sentence(), "拟好");
    }

    /// Toy: 3 nodes, 6 edges (the climb-plan "xianjin → 现金/先进" fixture).
    /// Without an LM, top-1 should be 现金 (highest individual freqs).
    /// With a mock LM that boosts "现金" and "先进", they should both
    /// surface in the top-2 — exactly the cross-path joint-scoring case
    /// the lattice is built to handle.
    #[test]
    fn xianjin_ambiguity_no_lm() {
        let mut g = Graph::for_buffer(7);
        // "xian" → 现 / 先 / 鲜
        g.add_edge(Edge::exact(0, 4, "现", -2.0));
        g.add_edge(Edge::exact(0, 4, "先", -2.5));
        g.add_edge(Edge::exact(0, 4, "鲜", -3.0));
        // "jin" → 金 / 进 / 京
        g.add_edge(Edge::exact(4, 7, "金", -2.0));
        g.add_edge(Edge::exact(4, 7, "进", -2.5));
        g.add_edge(Edge::exact(4, 7, "京", -3.0));

        let paths = g.viterbi(3, noop_lm);
        assert_eq!(paths.len(), 3);
        // Without LM, top-1 is by raw weight sum: 现+金 = -4.0
        assert_eq!(paths[0].sentence(), "现金");
        // Top-3 within the beam — exact ordering for ties up to score.
        for p in &paths {
            assert_eq!(p.words.len(), 2);
        }
    }

    #[test]
    fn xianjin_ambiguity_with_mock_lm() {
        let mut g = Graph::for_buffer(7);
        g.add_edge(Edge::exact(0, 4, "现", -2.0));
        g.add_edge(Edge::exact(0, 4, "先", -2.5));
        g.add_edge(Edge::exact(0, 4, "鲜", -3.0));
        g.add_edge(Edge::exact(4, 7, "金", -2.0));
        g.add_edge(Edge::exact(4, 7, "进", -2.5));
        g.add_edge(Edge::exact(4, 7, "京", -3.0));

        // Mock LM: heavy bonus for the two real-word pairs.
        let lm = |prev: &str, curr: &str| -> f32 {
            match (prev, curr) {
                ("现", "金") => 2.0, // 现金 score: -2.0 + -2.0 + 2.0 = -2.0
                ("先", "进") => 2.0, // 先进 score: -2.5 + -2.5 + 2.0 = -3.0
                _ => 0.0,
            }
        };

        let paths = g.viterbi(3, lm);
        let top_two: Vec<String> = paths.iter().take(2).map(Path::sentence).collect();
        // Both real-word pairs must surface in the top-2 thanks to the LM.
        assert!(top_two.contains(&"现金".to_string()),
            "expected 现金 in top-2, got {top_two:?}");
        assert!(top_two.contains(&"先进".to_string()),
            "expected 先进 in top-2, got {top_two:?}");
    }

    /// Cross-channel: an exact edge and a fuzzy edge at the same span,
    /// the fuzzy edge has a higher raw log_prob but eats the channel
    /// penalty, so the exact edge wins.
    #[test]
    fn fuzzy_channel_penalty_wins_against_higher_raw_prob() {
        let mut g = Graph::for_buffer(2);
        // Exact: "你" log_prob -1.5
        g.add_edge(Edge::exact(0, 2, "你", -1.5));
        // Fuzzy: "腻" log_prob -1.0 but channel -2.5 → total weight -3.5
        g.add_edge(Edge::fuzzy(0, 2, "腻", -1.0, -2.5));
        let paths = g.viterbi(3, noop_lm);
        assert_eq!(paths[0].sentence(), "你");
    }

    /// Buffer with no path covering the full length → empty result.
    #[test]
    fn no_complete_path_yields_empty() {
        let mut g = Graph::for_buffer(7);
        // Only "xian" 0..4, nothing covering 4..7.
        g.add_edge(Edge::exact(0, 4, "现", -1.0));
        let paths = g.viterbi(3, noop_lm);
        assert!(paths.is_empty());
    }

    /// CP-3.2: Path1aExact populate_lattice produces one Edge per
    /// dict candidate for a single-syllable buffer, covering the
    /// whole buffer span (0..n). Set of edge candidate words must
    /// equal the set returned by lookup_with_scores_into byte-for-byte.
    #[test]
    fn path1a_exact_matches_dict_lookup_set() {
        use crate::dict::PinyinDict;

        let dict = PinyinDict::embedded();
        let buf = "ni"; // single syllable

        // Old path: dict lookup directly.
        let mut scratch: Vec<(String, f64)> = Vec::new();
        dict.lookup_with_scores_into(buf, &mut scratch);
        let old_words: std::collections::BTreeSet<String> =
            scratch.iter().map(|(w, _)| w.clone()).collect();

        // New path: PathToLattice → Graph → enumerate edges.
        let mut g = Graph::for_buffer(buf.len());
        let n_added = Path1aExact.populate_lattice(buf, &dict, &mut g);
        let new_words: std::collections::BTreeSet<String> = g
            .edges()
            .iter()
            .map(|e| e.candidate.clone())
            .collect();

        assert_eq!(n_added, old_words.len(), "edge count must match dict lookup count");
        assert_eq!(old_words, new_words, "lattice edges must cover the same candidate set as the dict lookup");

        // Every edge must span the whole buffer (single-syllable case).
        for e in g.edges() {
            assert_eq!(e.from, 0);
            assert_eq!(e.to, buf.len());
            assert_eq!(e.kind, EdgeKind::Exact);
        }
    }

    /// CP-3.2: beam-Viterbi on a single-syllable lattice must produce
    /// the same top-K candidates the dict lookup would, in the same
    /// order (highest score first). This is the "USE_LATTICE=true vs
    /// false top-K diff < 5%" invariant the climb plan calls for.
    #[test]
    fn path1a_exact_viterbi_topk_matches_dict_topk() {
        use crate::dict::PinyinDict;

        let dict = PinyinDict::embedded();
        let buf = "ni";

        let mut scratch: Vec<(String, f64)> = Vec::new();
        dict.lookup_with_scores_into(buf, &mut scratch);
        // dict lookup order is FST iteration order, not score order;
        // sort by score desc to compare against lattice viterbi top-K.
        scratch.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(core::cmp::Ordering::Equal));
        let top_k_dict: Vec<String> =
            scratch.iter().take(3).map(|(w, _)| w.clone()).collect();

        let mut g = Graph::for_buffer(buf.len());
        Path1aExact.populate_lattice(buf, &dict, &mut g);
        let paths = g.viterbi(3, |_, _| 0.0);
        let top_k_lattice: Vec<String> = paths.iter().map(Path::sentence).collect();

        assert_eq!(
            top_k_dict, top_k_lattice,
            "lattice viterbi top-3 must match dict-sorted-by-score top-3"
        );
    }

    /// CP-3.3: `fuzzy_channel_log_prob` picks the correct per-pair
    /// value from the table and returns 0 for the canonical (no-swap)
    /// case.
    #[test]
    fn fuzzy_channel_log_prob_table_lookup() {
        // Initial-position swaps.
        assert_eq!(fuzzy_channel_log_prob("zi", "zhi"), -1.0);
        assert_eq!(fuzzy_channel_log_prob("zhi", "zi"), -1.0);
        assert_eq!(fuzzy_channel_log_prob("li", "ri"), -2.0); // r/l rarer
        assert_eq!(fuzzy_channel_log_prob("hua", "fua"), -2.0); // f/h rarer
        // Final-position swap.
        assert_eq!(fuzzy_channel_log_prob("xin", "xing"), -1.0);
        assert_eq!(fuzzy_channel_log_prob("ban", "bang"), -1.0);
        // Identity = 0.
        assert_eq!(fuzzy_channel_log_prob("ni", "ni"), 0.0);
        // Unrelated pair = penalised.
        assert_eq!(fuzzy_channel_log_prob("ni", "wo"), -10.0);
    }

    /// CP-3.3: Path1bFuzzy emits one Fuzzy edge per dict candidate per
    /// fuzzy variant. For `"zhi"` with z↔zh enabled, variants are
    /// `["zhi", "zi"]`; canonical "zhi" is skipped (Path 1a's job), so
    /// edges come from looking up "zi" only.
    #[test]
    fn path1b_fuzzy_zhi_zi_swap_produces_fuzzy_edges() {
        use crate::dict::PinyinDict;
        use crate::fuzzy::FuzzyConfig;

        let dict = PinyinDict::embedded();
        let buf = "zhi";
        let mut fuzzy = FuzzyConfig::strict();
        fuzzy.z_zh = true;
        let path = Path1bFuzzy { fuzzy };

        let mut g = Graph::for_buffer(buf.len());
        let n_added = path.populate_lattice(buf, &dict, &mut g);

        // The "zi" lookup must produce at least one candidate, given
        // it is a common syllable (字 / 自 / 子 / 紫 / 资 / 仔 etc).
        assert!(
            n_added > 0,
            "Path1bFuzzy expected to add edges for 'zi' variant of 'zhi'"
        );

        // Every emitted edge must be Fuzzy with channel = -1.0
        // (z↔zh swap penalty from FUZZY_CHANNEL_LOG_PROBS).
        for e in g.edges() {
            assert_eq!(e.from, 0);
            assert_eq!(e.to, buf.len());
            match e.kind {
                EdgeKind::Fuzzy(p) => assert!((p - (-1.0)).abs() < 1e-6),
                _ => panic!("Path1bFuzzy edge was not EdgeKind::Fuzzy: {:?}", e.kind),
            }
        }
    }

    /// CP-3.3: when fuzzy is OFF (strict config), Path1bFuzzy adds
    /// nothing — the only "variant" returned by FuzzyConfig::expand
    /// is the canonical syllable itself, which Path1bFuzzy skips.
    #[test]
    fn path1b_fuzzy_strict_config_adds_nothing() {
        use crate::dict::PinyinDict;
        use crate::fuzzy::FuzzyConfig;

        let dict = PinyinDict::embedded();
        let path = Path1bFuzzy { fuzzy: FuzzyConfig::strict() };

        let mut g = Graph::for_buffer(2);
        let n = path.populate_lattice("ni", &dict, &mut g);
        assert_eq!(n, 0, "strict fuzzy should add zero edges");
        assert!(g.edges().is_empty());
    }

    /// CP-3.3 cross-channel: at the same buffer span, an Exact edge
    /// for "你" and a Fuzzy edge for a competing word must rank with
    /// Exact preferred unless the Fuzzy candidate's dict log_prob
    /// outweighs the channel penalty.
    #[test]
    fn path1a_and_path1b_cross_channel_ranking() {
        let mut g = Graph::for_buffer(2);
        // Exact: 你 with mid log_prob.
        g.add_edge(Edge::exact(0, 2, "你", 5.0));
        // Fuzzy: 拟 with higher raw log_prob but -1.0 channel cost.
        // Effective weight: 5.5 + (-1.0) = 4.5 — still loses to Exact 5.0.
        g.add_edge(Edge::fuzzy(0, 2, "拟", 5.5, -1.0));

        let paths = g.viterbi(2, noop_lm);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].sentence(), "你");
        assert_eq!(paths[1].sentence(), "拟");
    }

    /// CP-3.4: Path1cTypo default channel log-prob = log10(0.05),
    /// i.e. ~5% P(typo | true).
    #[test]
    fn path1c_typo_default_channel_log_prob() {
        let p = Path1cTypo::default();
        // 10^(-1.301) = 0.05
        let prob = 10f32.powf(p.channel_log_prob);
        assert!((prob - 0.05).abs() < 1e-4, "default = log10(0.05), got prob {prob}");
    }

    /// CP-3.4: Path1cTypo.add_edges turns externally-resolved
    /// `(word, score)` pairs into EdgeKind::Typo edges spanning the
    /// whole buffer.
    #[test]
    fn path1c_typo_add_edges_emits_typo_kind() {
        let mut g = Graph::for_buffer(4);
        // Mock typo rescue: user typed "pyin" → "pinyin", let's say
        // 2 candidates from the cement IDF.
        let resolutions = vec![
            ("拼音".to_string(), 800_000.0),
            ("品音".to_string(), 50_000.0),
        ];
        let typo = Path1cTypo::default();
        let n = typo.add_edges("pyin", &resolutions, &mut g);
        assert_eq!(n, 2);
        for e in g.edges() {
            assert_eq!(e.from, 0);
            assert_eq!(e.to, 4);
            match e.kind {
                EdgeKind::Typo(p) => {
                    assert!((p - Path1cTypo::DEFAULT_CHANNEL_LOG_PROB).abs() < 1e-6);
                }
                _ => panic!("expected Typo, got {:?}", e.kind),
            }
        }
    }

    /// CP-3.4: empty resolutions add zero edges (graceful no-op when
    /// the typo cluster failed to resolve at the index layer).
    #[test]
    fn path1c_typo_empty_resolutions_no_op() {
        let mut g = Graph::for_buffer(4);
        let n = Path1cTypo::default().add_edges("pyin", &[], &mut g);
        assert_eq!(n, 0);
        assert!(g.edges().is_empty());
    }

    /// CP-3.4 cross-channel: Typo edge with higher raw log_prob still
    /// loses to a competing Exact edge if its channel penalty (~-1.3)
    /// outweighs the raw score difference.
    #[test]
    fn path1c_typo_loses_to_close_exact() {
        let mut g = Graph::for_buffer(4);
        // Exact 拼音 at log_prob 5.9 → weight 5.9
        g.add_edge(Edge::exact(0, 4, "拼音", 5.9));
        // Typo 品音 at log_prob 6.5 → weight 6.5 + (-1.301) = 5.199
        g.add_edge(Edge::typo(0, 4, "品音", 6.5, Path1cTypo::DEFAULT_CHANNEL_LOG_PROB));
        let paths = g.viterbi(2, noop_lm);
        assert_eq!(paths[0].sentence(), "拼音");
    }

    /// CP-3.5: Path2Abbrev default channel = log10(0.02) ≈ -1.699,
    /// i.e. 2% P(abbrev | full).
    #[test]
    fn path2_abbrev_default_channel_log_prob() {
        let p = Path2Abbrev::default();
        let prob = 10f32.powf(p.channel_log_prob);
        assert!((prob - 0.02).abs() < 1e-4, "default = log10(0.02), got prob {prob}");
    }

    /// CP-3.5: Path2Abbrev.add_edges emits Abbrev edges. Compare with
    /// Path1cTypo to confirm both use the same uniform-channel pattern
    /// but with the lower Abbrev probability (P=0.02 vs P=0.05).
    #[test]
    fn path2_abbrev_add_edges_emits_abbrev_kind_with_lower_p() {
        let mut g = Graph::for_buffer(2);
        let resolutions = vec![("中国".to_string(), 1_000_000.0)];
        let n = Path2Abbrev::default().add_edges("zg", &resolutions, &mut g);
        assert_eq!(n, 1);
        match g.edges()[0].kind {
            EdgeKind::Abbrev(p) => {
                assert!((p - Path2Abbrev::DEFAULT_CHANNEL_LOG_PROB).abs() < 1e-6);
                // Abbrev penalty must be stricter than Typo penalty
                // (lower probability → more negative log10).
                assert!(p < Path1cTypo::DEFAULT_CHANNEL_LOG_PROB);
            }
            _ => panic!("expected Abbrev, got {:?}", g.edges()[0].kind),
        }
    }

    /// CP-3.5: end-to-end cross-channel sanity. Four kinds of edges
    /// (Exact / Fuzzy / Typo / Abbrev) at the same span are ranked
    /// correctly by their cumulative weight = log_prob + channel.
    #[test]
    fn cross_channel_all_four_kinds_rank_correctly() {
        let mut g = Graph::for_buffer(2);
        // Match the climb-plan-suggested channel costs.
        g.add_edge(Edge::exact(0, 2, "exact", 5.0));
        g.add_edge(Edge::fuzzy(0, 2, "fuzzy", 5.5, -1.0));
        g.add_edge(Edge::typo(0, 2, "typo", 6.0, Path1cTypo::DEFAULT_CHANNEL_LOG_PROB));
        g.add_edge(Edge::abbrev(0, 2, "abbrev", 6.5, Path2Abbrev::DEFAULT_CHANNEL_LOG_PROB));

        // Compute expected effective weights.
        // exact  = 5.0
        // fuzzy  = 5.5 + -1.0  = 4.5
        // typo   = 6.0 + -1.301 = 4.699
        // abbrev = 6.5 + -1.699 = 4.801
        let paths = g.viterbi(4, noop_lm);
        assert_eq!(paths.len(), 4);
        let order: Vec<String> = paths.iter().map(Path::sentence).collect();
        assert_eq!(order, vec!["exact", "abbrev", "typo", "fuzzy"]);
    }

    // ------------------------------------------------------------------
    // CP-3.6 step-2 byte-equal gate (spec · 2026-06-16 sprint step #4)
    //
    // Contract pinned by this test:
    //   When the multi-syllable composition wire lands (Path 1a
    //   composition via the lattice rather than `PinyinDict::
    //   best_composition`'s standalone DP), the lattice's top-1 sentence
    //   for a multi-syllable buffer MUST equal what `best_composition`
    //   returns for the same buffer. Same dict, same LM, same input →
    //   same top-1 hanzi. Anything else means the wire's edge-weight
    //   encoding doesn't preserve the legacy DP's score ordering.
    //
    // Why this test is #[ignore]'d:
    //   The wire is not yet implemented — `multi_syllable_lattice_top1`
    //   below is an `unimplemented!()` stub. When the wire-developer
    //   adds the multi-syllable lattice path:
    //     1. Replace the stub body with a call into the new wire API
    //        (or inline-mirror its DP — whatever proves the wire's
    //         output, not just its existence).
    //     2. Remove `#[ignore]`.
    //     3. Run `cargo test --release multi_syllable_lattice_top1`
    //        with `--ignored` removed. Test must pass for every case.
    //        If it fails for a specific buffer, the wire's edge-weight
    //        encoding isn't preserving best_composition's ordering —
    //        fix the wire, not the test.
    //
    // Why not test it bit-equal:
    //   The test asserts top-1 hanzi parity, not numeric score parity.
    //   `best_composition` accumulates `raw_freq + bonuses - STEP_PENALTY`
    //   per segment; the lattice viterbi accumulates `edge.weight +
    //   lm(prev,curr)`. The wire can pick any edge-weight encoding that
    //   preserves the same ordering — log-domain, raw-domain, scaled,
    //   doesn't matter, as long as top-1 hanzi matches. That's the
    //   only invariant the host's candidate panel cares about.

    /// Byte-equal gate for the future CP-3.6 step-2 multi-syllable wire.
    /// See the section comment above for the contract this pins.
    #[test]
    #[ignore = "CP-3.6 step-2 multi-syllable composition wire pending — ungate when the wire lands and replace `multi_syllable_lattice_top1` stub"]
    fn multi_syllable_lattice_top1_matches_legacy_best_composition() {
        let dict = crate::dict::PinyinDict::embedded();

        // Representative multi-syllable buffers. Each must have a
        // `best_composition` hit (return Some) — if any fails to
        // resolve via the legacy path, the fixture is stale (dict
        // drifted) and the test is wrong, not the wire.
        let test_cases: &[&str] = &[
            "nihao",          // 2 syllables
            "wojiao",         // 2 syllables
            "nihaoma",        // 3 syllables
            "wodejia",        // 3 syllables
            "nihaomawojiao",  // 5 syllables — exercises deeper DP
        ];

        // Stub the wire-developer fills in. Until then, calling this
        // panics — but the `#[ignore]` attribute keeps the test out of
        // the default test run so the panic doesn't impact CI.
        fn multi_syllable_lattice_top1(
            _dict: &crate::dict::PinyinDict,
            _buf: &str,
        ) -> Option<String> {
            unimplemented!(
                "CP-3.6 step-2 multi-syllable lattice wire is not yet \
                 implemented. Replace this stub with a call into the new \
                 lattice composition path (see board CP-3.6 step-2 main \
                 task: \"multi-syllable composition (segmenter 接入 lattice)\")."
            )
        }

        for buf in test_cases {
            let legacy = dict
                .best_composition(buf)
                .unwrap_or_else(|| {
                    panic!(
                        "test fixture stale: legacy best_composition returned None \
                         for buf={buf:?}. Either the dict has drifted (update the \
                         fixture) or the buffer is below MIN_LEN (pick a longer one)."
                    )
                })
                .1;

            let lattice = multi_syllable_lattice_top1(&dict, buf).expect(
                "wire returned None — the wire should always produce SOME top-1 \
                 sentence for an input legacy can resolve",
            );

            assert_eq!(
                lattice, legacy,
                "BYTE-EQUAL GATE for buf={buf:?}: lattice multi-syllable top-1 \
                 {lattice:?} must equal legacy best_composition top-1 {legacy:?}. \
                 The wire's edge-weight encoding isn't preserving \
                 best_composition's DP ordering for this input."
            );
        }
    }

    /// Beam pruning actually limits the partial-path explosion. Build a
    /// fan-out lattice where naive Viterbi would keep `3^k` partials at
    /// node k; with beam=2 the result must still be the top-2 globally.
    #[test]
    fn beam_pruning_keeps_only_top_k() {
        let mut g = Graph::for_buffer(6);
        for (i, word) in ["a", "b", "c"].iter().enumerate() {
            // log_prob = -1.0 * (i+1)
            g.add_edge(Edge::exact(0, 2, *word, -1.0 * (i + 1) as f32));
            g.add_edge(Edge::exact(2, 4, *word, -1.0 * (i + 1) as f32));
            g.add_edge(Edge::exact(4, 6, *word, -1.0 * (i + 1) as f32));
        }
        let paths = g.viterbi(2, noop_lm);
        assert_eq!(paths.len(), 2);
        // Globally best path is "aaa" at -3.0.
        assert_eq!(paths[0].sentence(), "aaa");
        // Beam=2 means second-best is "aab" or "aba" or "baa", all at -4.0.
        assert!((paths[1].score - (-4.0)).abs() < 1e-6);
    }
}
