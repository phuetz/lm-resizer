// Derive the packed counting vocabulary from the pinned reference dependency.
// No downloaded data, duplicated vocabulary in Git, or personal build paths.
use std::{
    env,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
};
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        return;
    }
    let bpe = tiktoken_rs::o200k_base().expect("reference o200k vocabulary");
    let path =
        PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR")).join("o200k-count.bin");
    let mut output = BufWriter::new(File::create(path).expect("packed vocabulary file"));
    // Fixed little-endian open-addressed index. Build once, borrow at runtime;
    // no 200,000 heap allocations and no hash-table reconstruction per CLI.
    let mut slots = vec![(u32::MAX, 0_u32); 1 << 19];
    let mut offset = 4_u32;
    // tiktoken-rs =0.11.0: ordinary ranks 0..199998; special ranks excluded.
    const RANKS: u32 = 199_998;
    output.write_all(&RANKS.to_le_bytes()).unwrap();
    for rank in 0..RANKS {
        let bytes = bpe.decode_bytes(&[rank]).expect("ordinary rank");
        let length = u16::try_from(bytes.len()).expect("token byte length");
        let hash = bytes.iter().fold(0xcbf29ce484222325_u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        });
        let mut slot = hash as usize & (slots.len() - 1);
        while slots[slot].0 != u32::MAX {
            slot = (slot + 1) & (slots.len() - 1);
        }
        slots[slot] = (offset, rank);
        offset += 2 + u32::from(length);
        output.write_all(&length.to_le_bytes()).unwrap();
        output.write_all(&bytes).unwrap();
    }
    output.flush().unwrap();
    let index_path = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("o200k-index.bin");
    let mut index = BufWriter::new(File::create(index_path).unwrap());
    for (offset, rank) in slots {
        index.write_all(&offset.to_le_bytes()).unwrap();
        index.write_all(&rank.to_le_bytes()).unwrap();
    }
    index.flush().unwrap();
}
