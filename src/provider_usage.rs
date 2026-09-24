//! Token counts the provider itself reported, kept apart from our estimates.
//!
//! The proxy has always *estimated* what it saved: bytes removed divided by
//! four, plus fixed hypotheses for verbosity and effort. Those figures are
//! labelled as such, but the provider's own `usage` block — the only counters
//! a bill is computed from — was never read. This module reads it, verbatim,
//! and never mixes it into an estimate:
//!
//! - `raw` is the provider's `usage` object exactly as received;
//! - the normalised fields say which convention they follow, because
//!   Anthropic's `input_tokens` excludes cached tokens while OpenAI's
//!   `prompt_tokens` includes them;
//! - nothing here is a price. Counting tokens is not billing them.
//!
//! Streams report usage in events (`message_start`, `message_delta`, a final
//! OpenAI chunk). A stream can also be abandoned by the client halfway: the
//! record then says `stream_completed: false` rather than pretending the
//! counts are final.

use std::pin::Pin;
use std::task::{Context, Poll};

use axum::body::Bytes;
use futures_util::Stream;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProviderUsage {
    /// `provider-response` or `provider-stream`.
    pub source: String,
    /// `anthropic` (input excludes cache reads/writes) or `openai` (input
    /// includes cached tokens).
    pub convention: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    /// The provider's `usage` object(s), untouched.
    pub raw: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_completed: Option<bool>,
}

fn u(v: &Value, path: &[&str]) -> Option<u64> {
    let mut cur = v;
    for key in path {
        cur = cur.get(*key)?;
    }
    cur.as_u64()
}

/// Merge one provider `usage` object into `acc`. Later values win: an
/// Anthropic `message_delta` carries the final `output_tokens`.
fn merge_usage(acc: &mut ProviderUsage, usage: &Value) {
    if !usage.is_object() {
        return;
    }
    acc.raw.push(usage.clone());
    if usage.get("prompt_tokens").is_some() || usage.get("completion_tokens").is_some() {
        acc.convention = "openai".into();
        acc.input_tokens = u(usage, &["prompt_tokens"]).or(acc.input_tokens);
        acc.output_tokens = u(usage, &["completion_tokens"]).or(acc.output_tokens);
        acc.cache_read_input_tokens =
            u(usage, &["prompt_tokens_details", "cached_tokens"]).or(acc.cache_read_input_tokens);
        return;
    }
    if usage.get("input_tokens_details").is_some() {
        // OpenAI Responses API: input_tokens includes cached_tokens.
        acc.convention = "openai".into();
        acc.cache_read_input_tokens =
            u(usage, &["input_tokens_details", "cached_tokens"]).or(acc.cache_read_input_tokens);
    } else if acc.convention.is_empty() {
        acc.convention = "anthropic".into();
    }
    acc.input_tokens = u(usage, &["input_tokens"]).or(acc.input_tokens);
    acc.output_tokens = u(usage, &["output_tokens"]).or(acc.output_tokens);
    acc.cache_read_input_tokens =
        u(usage, &["cache_read_input_tokens"]).or(acc.cache_read_input_tokens);
    acc.cache_creation_input_tokens =
        u(usage, &["cache_creation_input_tokens"]).or(acc.cache_creation_input_tokens);
}

/// Usage from a complete (non-streaming) provider response.
pub fn from_response(body: &Value) -> Option<ProviderUsage> {
    let usage = body.get("usage")?;
    let mut acc = ProviderUsage {
        source: "provider-response".into(),
        ..Default::default()
    };
    merge_usage(&mut acc, usage);
    (!acc.raw.is_empty()).then_some(acc)
}

/// Incremental reader of `data:` lines in a server-sent event stream.
#[derive(Debug, Default)]
pub struct SseUsageScanner {
    pending: Vec<u8>,
    usage: ProviderUsage,
}

impl SseUsageScanner {
    pub fn feed(&mut self, chunk: &[u8]) {
        self.pending.extend_from_slice(chunk);
        while let Some(pos) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=pos).collect();
            self.line(&line);
        }
        // An event line longer than this is not a usage event worth keeping in
        // memory; drop it rather than grow without bound.
        if self.pending.len() > 1 << 20 {
            self.pending.clear();
        }
    }

    fn line(&mut self, raw: &[u8]) {
        let Ok(text) = std::str::from_utf8(raw) else {
            return;
        };
        let Some(data) = text.trim_end().strip_prefix("data:") else {
            return;
        };
        let Ok(event) = serde_json::from_str::<Value>(data.trim()) else {
            return;
        };
        if let Some(usage) = event.pointer("/message/usage") {
            merge_usage(&mut self.usage, usage); // Anthropic message_start
        }
        if let Some(usage) = event.get("usage") {
            merge_usage(&mut self.usage, usage); // message_delta, OpenAI final chunk
        }
        if let Some(usage) = event.pointer("/response/usage") {
            merge_usage(&mut self.usage, usage); // OpenAI Responses `response.completed`
        }
    }

    pub fn finish(mut self, completed: bool) -> Option<ProviderUsage> {
        if !self.pending.is_empty() {
            let rest = std::mem::take(&mut self.pending);
            self.line(&rest);
        }
        if self.usage.raw.is_empty() {
            return None;
        }
        self.usage.source = "provider-stream".into();
        self.usage.stream_completed = Some(completed);
        Some(self.usage)
    }
}

type Finish = Box<dyn FnOnce(Option<ProviderUsage>, bool) + Send>;

/// Pass a byte stream through untouched while reading its usage events.
/// `on_finish` runs exactly once: at the end of the stream, or when the stream
/// is dropped early (client gone), with `completed = false`.
pub struct UsageTap<S> {
    inner: S,
    scanner: Option<SseUsageScanner>,
    completed: bool,
    on_finish: Option<Finish>,
}

impl<S> UsageTap<S> {
    pub fn new(inner: S, on_finish: Finish) -> Self {
        Self {
            inner,
            scanner: Some(SseUsageScanner::default()),
            completed: false,
            on_finish: Some(on_finish),
        }
    }

    fn fire(&mut self) {
        if let (Some(scanner), Some(f)) = (self.scanner.take(), self.on_finish.take()) {
            f(scanner.finish(self.completed), self.completed);
        }
    }
}

impl<S, E> Stream for UsageTap<S>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
{
    type Item = Result<Bytes, E>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = &mut *self;
        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                if let Some(scanner) = this.scanner.as_mut() {
                    scanner.feed(&chunk);
                }
                Poll::Ready(Some(Ok(chunk)))
            }
            Poll::Ready(None) => {
                this.completed = true;
                this.fire();
                Poll::Ready(None)
            }
            other => other,
        }
    }
}

impl<S> Drop for UsageTap<S> {
    fn drop(&mut self) {
        self.fire();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    #[test]
    fn anthropic_garde_les_compteurs_de_cache_tels_quels() {
        let body = json!({"usage": {"input_tokens": 12, "output_tokens": 40,
            "cache_read_input_tokens": 3000, "cache_creation_input_tokens": 200}});
        let u = from_response(&body).unwrap();
        assert_eq!(u.convention, "anthropic");
        assert_eq!((u.input_tokens, u.output_tokens), (Some(12), Some(40)));
        assert_eq!(u.cache_read_input_tokens, Some(3000));
        assert_eq!(u.cache_creation_input_tokens, Some(200));
        assert_eq!(
            u.raw,
            vec![body["usage"].clone()],
            "brut conservé à l'identique"
        );
    }

    #[test]
    fn openai_signale_que_l_entree_inclut_le_cache() {
        let body = json!({"usage": {"prompt_tokens": 1000, "completion_tokens": 7,
            "prompt_tokens_details": {"cached_tokens": 896}}});
        let u = from_response(&body).unwrap();
        assert_eq!(u.convention, "openai");
        assert_eq!(u.input_tokens, Some(1000));
        assert_eq!(u.cache_read_input_tokens, Some(896));
        assert_eq!(u.cache_creation_input_tokens, None);
    }

    #[test]
    fn sans_bloc_usage_rien_n_est_invente() {
        assert!(from_response(&json!({"id": "x", "content": []})).is_none());
        assert!(SseUsageScanner::default().finish(true).is_none());
    }

    #[test]
    fn flux_anthropic_decoupe_n_importe_ou() {
        let sse = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":9,\"cache_read_input_tokens\":500,\"output_tokens\":1}}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"ok\"}}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":33}}\n\n";
        let mut s = SseUsageScanner::default();
        for piece in sse.as_bytes().chunks(7) {
            s.feed(piece);
        }
        let u = s.finish(true).unwrap();
        assert_eq!(u.input_tokens, Some(9));
        assert_eq!(u.cache_read_input_tokens, Some(500));
        assert_eq!(
            u.output_tokens,
            Some(33),
            "le message_delta final l'emporte"
        );
        assert_eq!(u.stream_completed, Some(true));
    }

    #[tokio::test]
    async fn un_flux_abandonne_est_marque_incomplet() {
        let seen: Arc<Mutex<Option<(Option<ProviderUsage>, bool)>>> = Arc::default();
        let s2 = seen.clone();
        let chunks: Vec<Result<Bytes, std::io::Error>> = vec![
            Ok(Bytes::from_static(b"data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":5}}}\n")),
            Ok(Bytes::from_static(b"data: {\"type\":\"content_block_delta\"}\n")),
        ];
        let mut tap = UsageTap::new(
            futures_util::stream::iter(chunks),
            Box::new(move |u, c| *s2.lock().unwrap() = Some((u, c))),
        );
        let first = tap.next().await.unwrap().unwrap();
        assert!(first.starts_with(b"data:"), "octets transmis tels quels");
        drop(tap); // le client coupe
        let (usage, completed) = seen.lock().unwrap().take().unwrap();
        assert!(!completed);
        assert_eq!(usage.unwrap().stream_completed, Some(false));
    }
}
