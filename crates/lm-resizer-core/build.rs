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
    // tiktoken-rs =0.11.0: ordinary ranks 0..199998; special ranks excluded.
    const RANKS: u32 = 199_998;
    output.write_all(&RANKS.to_le_bytes()).unwrap();
    for rank in 0..RANKS {
        let bytes = bpe.decode_bytes(&[rank]).expect("ordinary rank");
        let length = u16::try_from(bytes.len()).expect("token byte length");
        output.write_all(&length.to_le_bytes()).unwrap();
        output.write_all(&bytes).unwrap();
    }
    output.flush().unwrap();
}
