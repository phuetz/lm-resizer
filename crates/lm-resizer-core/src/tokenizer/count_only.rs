//! Exact o200k counting without decoder tables or an unused sorted vocabulary.
//! The build script packs the pinned tiktoken-rs vocabulary; regex and BPE
//! merges still come from that reference implementation.
use fancy_regex::Regex;
use std::sync::LazyLock;
use tiktoken_rs::{Rank, O200K_BASE_PAT_STR};

static PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(O200K_BASE_PAT_STR).expect("o200k regex"));
// On ASCII input the Unicode categories reduce exactly to these ranges.
// Derive from the pinned pattern, preserving branch order and lookarounds.
// This avoids compiling large Unicode character tables for ordinary CLI text.
static ASCII_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    let mut pattern = O200K_BASE_PAT_STR.to_owned();
    for (category, range) in [
        (r"\p{Lu}", "A-Z"),
        (r"\p{Ll}", "a-z"),
        (r"\p{L}", "A-Za-z"),
        (r"\p{N}", "[0-9]"),
        (r"\p{Lt}", ""),
        (r"\p{Lm}", ""),
        (r"\p{Lo}", ""),
        (r"\p{M}", ""),
        (r"\s", r"[\x09-\x0D ]"),
        (r"\S", r"[^\x09-\x0D ]"),
    ] {
        pattern = pattern.replace(category, range);
    }
    Regex::new(&pattern).expect("ASCII count pattern")
});
thread_local! {
    // Independent regex search caches avoid contention across counting workers.
    static THREAD_PATTERN: Regex = PATTERN.clone();
    static ASCII_THREAD_PATTERN: Regex = ASCII_PATTERN.clone();
}

// Explicit byte decoding is alignment- and endianness-independent. The index
// is generated from the pinned vocabulary by build.rs, never downloaded.
struct RankTable;
impl RankTable {
    fn get(&self, bytes: &[u8]) -> Option<Rank> {
        const INDEX: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/o200k-index.bin"));
        const PACKED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/o200k-count.bin"));
        let hash = bytes.iter().fold(0xcbf29ce484222325_u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        });
        let mask = INDEX.len() / 8 - 1;
        let mut slot = hash as usize & mask;
        loop {
            let entry = &INDEX[slot * 8..slot * 8 + 8];
            let offset = u32::from_le_bytes(entry[..4].try_into().unwrap());
            if offset == u32::MAX {
                return None;
            }
            let offset = offset as usize;
            let len = u16::from_le_bytes(PACKED[offset..offset + 2].try_into().unwrap()) as usize;
            if &PACKED[offset + 2..offset + 2 + len] == bytes {
                return Some(u32::from_le_bytes(entry[4..].try_into().unwrap()));
            }
            slot = (slot + 1) & mask;
        }
    }
}

pub(super) struct CountOnlyBpe {
    ranks: RankTable,
}

impl CountOnlyBpe {
    pub(super) fn new() -> Self {
        Self { ranks: RankTable }
    }

    // Greedy BPE: merge the lowest-rank adjacent pair, leftmost on ties.
    // Small pieces need only one allocation; large pieces use the existing heap.
    fn count_small(&self, piece: &[u8]) -> usize {
        let mut boundaries: Vec<_> = (0..=piece.len()).collect();
        loop {
            let next = boundaries
                .windows(3)
                .enumerate()
                .filter_map(|(i, b)| self.ranks.get(&piece[b[0]..b[2]]).map(|rank| (rank, i)))
                .min();
            let Some((_, i)) = next else {
                return boundaries.len() - 1;
            };
            boundaries.remove(i + 1);
        }
    }

    pub(super) fn count(&self, text: &str) -> usize {
        if text.is_ascii() {
            ASCII_THREAD_PATTERN.with(|pattern| self.count_with_pattern(text, pattern))
        } else {
            THREAD_PATTERN.with(|pattern| self.count_with_pattern(text, pattern))
        }
    }

    fn count_with_pattern(&self, text: &str, pattern: &Regex) -> usize {
        pattern
            .find_iter(text)
            .map(|matched| {
                let piece = matched.expect("o200k match").as_str().as_bytes();
                if self.ranks.get(piece).is_some() {
                    1
                } else if piece.len() > 64 {
                    self.count_heap(piece)
                } else {
                    self.count_small(piece)
                }
            })
            .sum()
    }
    /// Same greedy merge order as tiktoken_rs::byte_pair_split: minimum rank,
    /// then leftmost byte offset. A linked list and stale-checked priority queue
    /// replace repeated full scans and vector removals on a long regex piece.
    /// This is global BPE, not fixed-size chunking or a token-count estimate.
    fn count_heap(&self, piece: &[u8]) -> usize {
        use std::cmp::Reverse;
        use std::collections::BinaryHeap;
        let n = piece.len();
        if n < 2 {
            return n;
        }
        let mut next: Vec<usize> = (1..=n).collect();
        let mut prev: Vec<usize> = (0..n).map(|i| i.wrapping_sub(1)).collect();
        let mut queue = BinaryHeap::new();
        for left in 0..n - 1 {
            if let Some(rank) = self.ranks.get(&piece[left..left + 2]) {
                queue.push(Reverse((rank, left, left + 2)));
            }
        }
        let mut count = n;
        while let Some(Reverse((_, left, end))) = queue.pop() {
            let middle = next[left];
            // Merges only grow ranges. An unchanged two-node endpoint means
            // both the text and its rank are unchanged; deleted nodes are marked.
            if middle >= n || next[middle] != end {
                continue;
            }
            next[left] = end;
            next[middle] = usize::MAX;
            if end < n {
                prev[end] = left;
            }
            count -= 1;
            for start in [prev[left], left] {
                if start >= n {
                    continue;
                }
                let middle = next[start];
                if middle >= n {
                    continue;
                }
                let end = next[middle];
                if let Some(rank) = self.ranks.get(&piece[start..end]) {
                    queue.push(Reverse((rank, start, end)));
                }
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiktoken_rs::byte_pair_split;
    fn reference_ranks() -> rustc_hash::FxHashMap<Vec<u8>, Rank> {
        let bpe = tiktoken_rs::o200k_base().unwrap();
        (0..199_998)
            .map(|rank| (bpe.decode_bytes(&[rank]).unwrap(), rank))
            .collect()
    }
    #[test]
    fn static_index_matches_every_reference_rank_and_short_merges() {
        let ranks = reference_ranks();
        let bpe = CountOnlyBpe::new();
        for (bytes, rank) in &ranks {
            assert_eq!(bpe.ranks.get(bytes), Some(*rank));
        }
        assert_eq!(bpe.ranks.get(b""), None);
        let mut seed = 913_u64;
        for len in 2..65 {
            for _ in 0..20 {
                let input: Vec<_> = (0..len)
                    .map(|_| {
                        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                        (seed >> 32) as u8
                    })
                    .collect();
                assert_eq!(
                    bpe.count_small(&input),
                    byte_pair_split(&input, &ranks).len()
                );
            }
        }
    }
    #[test]
    fn heap_matches_reference_merge_order_including_rank_ties() {
        let bpe = CountOnlyBpe::new();
        let ranks = reference_ranks();
        for pattern in ["x", " ", "0", "é漢_", "abababa", "aaab", "\0"] {
            for length in [1, 2, 3, 31, 4095, 4096, 8193] {
                let input = pattern.repeat(length);
                assert_eq!(
                    bpe.count_heap(input.as_bytes()),
                    if input.len() == 1 {
                        1
                    } else {
                        byte_pair_split(input.as_bytes(), &ranks).len()
                    },
                    "{pattern:?}, {length}"
                );
            }
        }
        let mut seed = 42_u64;
        for _ in 0..40 {
            let input: Vec<u8> = (0..5000)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    b"abcXYZ012_ "[(seed >> 32) as usize % 10]
                })
                .collect();
            assert_eq!(
                bpe.count_heap(&input),
                byte_pair_split(&input, &ranks).len()
            );
        }
    }
}
