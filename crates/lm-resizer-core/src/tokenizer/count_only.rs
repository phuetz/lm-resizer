//! Exact o200k counting without decoder tables or an unused sorted vocabulary.
//! The build script packs the pinned tiktoken-rs vocabulary; regex and BPE
//! merges still come from that reference implementation.
use fancy_regex::Regex;
use rustc_hash::FxHashMap;
use std::sync::LazyLock;
use tiktoken_rs::{byte_pair_split, Rank, O200K_BASE_PAT_STR};

static PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(O200K_BASE_PAT_STR).expect("o200k regex"));
thread_local! {
    // Independent regex search caches avoid contention across counting workers.
    static THREAD_PATTERN: Regex = PATTERN.clone();
}

pub(super) struct CountOnlyBpe {
    ranks: FxHashMap<Vec<u8>, Rank>,
}

impl CountOnlyBpe {
    pub(super) fn new() -> Self {
        let packed = include_bytes!(concat!(env!("OUT_DIR"), "/o200k-count.bin"));
        let count = u32::from_le_bytes(packed[..4].try_into().unwrap());
        let mut ranks = FxHashMap::with_capacity_and_hasher(count as usize, Default::default());
        let mut offset = 4;
        for rank in 0..count {
            let length =
                u16::from_le_bytes(packed[offset..offset + 2].try_into().unwrap()) as usize;
            offset += 2;
            ranks.insert(packed[offset..offset + length].to_vec(), rank);
            offset += length;
        }
        assert_eq!(offset, packed.len());
        Self { ranks }
    }

    pub(super) fn count(&self, text: &str) -> usize {
        THREAD_PATTERN.with(|pattern| {
            pattern
                .find_iter(text)
                .map(|matched| {
                    let piece = matched.expect("o200k match").as_str().as_bytes();
                    if self.ranks.contains_key(piece) {
                        1
                    } else {
                        byte_pair_split(piece, &self.ranks).len()
                    }
                })
                .sum()
        })
    }
}
