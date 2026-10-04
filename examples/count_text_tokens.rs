//! Recount archived UTF-8 files with the reference tokenizer, independently
//! of the CLI reports/history. Usage: cargo run --example count_text_tokens -- FILE...
use anyhow::Result;
use lm_resizer_core::tokenizer::{TiktokenCounter, Tokenizer};

fn main() -> Result<()> {
    let counter = TiktokenCounter::for_model("gpt-4o")?;
    println!(
        "tokenizer: tiktoken-rs/{} (encode_ordinary)",
        counter.encoding_name()
    );
    println!("bytes\ttokens\tfile");
    for path in std::env::args().skip(1) {
        let text = std::fs::read_to_string(&path)?;
        println!("{}\t{}\t{}", text.len(), counter.count_text(&text), path);
    }
    Ok(())
}
