use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::oneshot;

/// Wraps an upstream SSE stream, forwarding bytes to the HTTP client
/// while accumulating them in a buffer. When the stream ends, sends
/// the complete buffer via a oneshot channel for DB storage.
pub struct SseTeeStream {
    inner: Pin<Box<dyn Stream<Item = reqwest::Result<Bytes>> + Send>>,
    buffer: Vec<u8>,
    done_tx: Option<oneshot::Sender<Vec<u8>>>,
}

impl SseTeeStream {
    pub fn new(
        inner: impl Stream<Item = reqwest::Result<Bytes>> + Send + 'static,
        done_tx: oneshot::Sender<Vec<u8>>,
    ) -> Self {
        SseTeeStream {
            inner: Box::pin(inner),
            buffer: Vec::new(),
            done_tx: Some(done_tx),
        }
    }
}

impl Stream for SseTeeStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(chunk))) => {
                self.buffer.extend_from_slice(&chunk);
                Poll::Ready(Some(Ok(chunk)))
            }
            Poll::Ready(Some(Err(e))) => {
                if let Some(tx) = self.done_tx.take() {
                    let _ = tx.send(std::mem::take(&mut self.buffer));
                }
                Poll::Ready(Some(Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    e.to_string(),
                ))))
            }
            Poll::Ready(None) => {
                if let Some(tx) = self.done_tx.take() {
                    let _ = tx.send(std::mem::take(&mut self.buffer));
                }
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Parse accumulated SSE bytes and extract the combined text content.
/// Returns `(accumulated_text, input_tokens, output_tokens)`.
pub fn parse_sse_content(data: &[u8]) -> (String, Option<i64>, Option<i64>) {
    let text = String::from_utf8_lossy(data);
    let mut content = String::new();
    let mut input_tokens: Option<i64> = None;
    let mut output_tokens: Option<i64> = None;

    for event_block in text.split("\n\n") {
        let event_block = event_block.trim();
        if event_block.is_empty() {
            continue;
        }

        let mut event_type = String::new();
        let mut data_line = String::new();

        for line in event_block.lines() {
            if let Some(val) = line.strip_prefix("event: ") {
                event_type = val.trim().to_string();
            } else if let Some(val) = line.strip_prefix("data: ") {
                data_line = val.trim().to_string();
            }
        }

        if data_line == "[DONE]" || data_line.is_empty() {
            continue;
        }

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&data_line) {
            match event_type.as_str() {
                "content_block_delta" => {
                    if let Some(delta_text) = json
                        .get("delta")
                        .and_then(|d| d.get("text"))
                        .and_then(|t| t.as_str())
                    {
                        content.push_str(delta_text);
                    }
                }
                "message_delta" => {
                    if let Some(usage) = json.get("usage") {
                        if let Some(out) = usage.get("output_tokens").and_then(|v| v.as_i64()) {
                            output_tokens = Some(out);
                        }
                    }
                }
                "message_start" => {
                    if let Some(usage) = json
                        .get("message")
                        .and_then(|m| m.get("usage"))
                    {
                        if let Some(inp) = usage.get("input_tokens").and_then(|v| v.as_i64()) {
                            input_tokens = Some(inp);
                        }
                        if let Some(out) = usage.get("output_tokens").and_then(|v| v.as_i64()) {
                            output_tokens = Some(out);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    (content, input_tokens, output_tokens)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use tokio::sync::oneshot;

    // Build a fake reqwest stream from a list of byte chunks
    fn make_stream(
        chunks: Vec<&'static [u8]>,
    ) -> impl Stream<Item = reqwest::Result<Bytes>> + Send + 'static {
        futures::stream::iter(
            chunks
                .into_iter()
                .map(|c| Ok(Bytes::from_static(c))),
        )
    }

    #[tokio::test]
    async fn tee_forwards_all_chunks_to_consumer() {
        let (done_tx, _done_rx) = oneshot::channel();
        let stream = make_stream(vec![b"chunk1", b"chunk2", b"chunk3"]);
        let mut tee = SseTeeStream::new(stream, done_tx);

        let mut received = Vec::new();
        while let Some(chunk) = tee.next().await {
            received.push(chunk.unwrap());
        }

        assert_eq!(received.len(), 3);
        assert_eq!(received[0], Bytes::from_static(b"chunk1"));
        assert_eq!(received[2], Bytes::from_static(b"chunk3"));
    }

    #[tokio::test]
    async fn tee_sends_accumulated_buffer_on_completion() {
        let (done_tx, done_rx) = oneshot::channel();
        let stream = make_stream(vec![b"hello", b" ", b"world"]);
        let mut tee = SseTeeStream::new(stream, done_tx);

        while tee.next().await.is_some() {}

        let buf = done_rx.await.unwrap();
        assert_eq!(buf, b"hello world");
    }

    #[tokio::test]
    async fn tee_sends_buffer_on_empty_stream() {
        let (done_tx, done_rx) = oneshot::channel();
        let stream = make_stream(vec![]);
        let mut tee = SseTeeStream::new(stream, done_tx);

        while tee.next().await.is_some() {}

        let buf = done_rx.await.unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn parse_sse_empty_input() {
        let (content, inp, out) = parse_sse_content(b"");
        assert!(content.is_empty());
        assert!(inp.is_none());
        assert!(out.is_none());
    }

    #[test]
    fn parse_sse_extracts_content_block_delta() {
        let sse = b"event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n\
                    event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" world\"}}\n\n";
        let (content, _, _) = parse_sse_content(sse);
        assert_eq!(content, "Hello world");
    }

    #[test]
    fn parse_sse_extracts_tokens_from_message_start() {
        let sse = b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":42,\"output_tokens\":0}}}\n\n";
        let (_, inp, out) = parse_sse_content(sse);
        assert_eq!(inp, Some(42));
        assert_eq!(out, Some(0));
    }

    #[test]
    fn parse_sse_extracts_output_tokens_from_message_delta() {
        let sse = b"event: message_delta\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":17}}\n\n";
        let (_, _, out) = parse_sse_content(sse);
        assert_eq!(out, Some(17));
    }

    #[test]
    fn parse_sse_ignores_done_sentinel() {
        let sse = b"data: [DONE]\n\n";
        let (content, inp, out) = parse_sse_content(sse);
        assert!(content.is_empty());
        assert!(inp.is_none());
        assert!(out.is_none());
    }

    #[test]
    fn parse_sse_ignores_unknown_events() {
        let sse = b"event: ping\ndata: {\"type\":\"ping\"}\n\n";
        let (content, inp, out) = parse_sse_content(sse);
        assert!(content.is_empty());
        assert!(inp.is_none());
        assert!(out.is_none());
    }

    #[test]
    fn parse_sse_full_stream_roundtrip() {
        let sse = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":3}}\n\n",
            "data: [DONE]\n\n",
        );
        let (content, inp, out) = parse_sse_content(sse.as_bytes());
        assert_eq!(content, "Hi");
        assert_eq!(inp, Some(10));
        assert_eq!(out, Some(3));
    }
}
