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
