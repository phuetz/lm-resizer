use lm_resizer_core::LmResizer;

fn main() {
    let tool_output = r#"{
  "events": [
    { "level": "info", "message": "starting build" },
    { "level": "info", "message": "starting build" },
    { "level": "error", "message": "compile failed" }
  ]
}"#;

    let report = LmResizer::new().compress(tool_output, "summarize build output");

    println!("content_type={}", report.content_type);
    println!("original_bytes={}", report.original_bytes);
    println!("compressed_bytes={}", report.compressed_bytes);
    println!("bytes_saved={}", report.bytes_saved);
    println!("steps={}", report.steps_applied.join(","));
    println!("{}", report.output);
}
