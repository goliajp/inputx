//! `inputx-dict-format` — IDFv1 binary dict format for IME engines.
//!
//! Probability-native (Q4 fixed-point log priors per
//! [`inputx_scoring`]), mmap zero-copy reader, deterministic writer.
//! Same binary layout across pinyin / wubi / Japanese / future Korean
//! and Vietnamese engines.
//!
//! # Architecture (per `.claude/PLAN-dict-format-IDFv1.md`)
//!
//! ```text
//! +---------+---------------+--------------+---------------+----------------+
//! | Header  | String pool   | Entry table  | FST code idx  | FST word idx   |
//! | 64 B    | varlen, pad8  | N × 16 B     | varlen        | varlen         |
//! +---------+---------------+--------------+---------------+----------------+
//!                                          | Bigram block (optional)        |
//!                                          | Embedding block (optional)     |
//!                                          | Padding to 8-byte EOF          |
//!                                          +--------------------------------+
//! ```
//!
//! - **Header**: magic `b"IDFv"`, format_version (currently 1), section
//!   offsets, sha256 of payload.
//! - **String pool**: deduplicated UTF-8 with byte offsets.
//! - **Entry table**: fixed 16 B per entry; carries word_offset (u24),
//!   code_offset (u24), log_prior (i16 Q4), match_type (u8), flags (u8),
//!   bigram_offset (u32, 0 if absent), embedding_offset (u32, 0 if
//!   absent).
//! - **FST code index**: [`inputx_fsa::Fsa`] mapping code bytes →
//!   entry_index (first hit; multi-reading entries follow as a run).
//! - **FST word index**: reverse, word → entry_index, for L0 / blacklist
//!   joins.
//!
//! # Reader / writer
//!
//! - [`reader::IdfReader::open`] — mmap a `.idf` file and validate header.
//! - [`reader::IdfReader::lookup`] — exact code → iterator of entries.
//! - [`reader::IdfReader::prefix_top_k`] — prefix scan top-k by log_prior.
//! - [`writer::IdfBuilder`] (gated on `std`) — deterministic build:
//!   sort + dedupe entries, build FST, write atomic via tmpfile + rename.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;

pub mod codec;
pub mod reader;
#[cfg(feature = "std")]
pub mod writer;

pub use codec::{ENTRY_SIZE, EngineKind, EntryFlags, HEADER_SIZE, Header, MAGIC, Version};
pub use reader::{Entry, IdfReader};
#[cfg(feature = "std")]
pub use writer::IdfBuilder;
