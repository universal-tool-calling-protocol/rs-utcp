use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::mpsc;

/// A minimal streaming abstraction that mirrors go-utcp's StreamResult (Next/Close).
#[async_trait]
pub trait StreamResult: Send {
    /// Pull the next value from the stream. Returns Ok(None) on EOF.
    async fn next(&mut self) -> Result<Option<Value>>;
    /// Close the stream and release any underlying resources.
    async fn close(&mut self) -> Result<()>;
}

/// StreamResult backed by a channel of `Result<Value>`.
pub struct ChannelStreamResult {
    rx: mpsc::Receiver<Result<Value>>,
    close_fn: Option<Box<dyn FnOnce() -> Result<()> + Send>>,
}

impl ChannelStreamResult {
    pub fn new(
        rx: mpsc::Receiver<Result<Value>>,
        close_fn: Option<Box<dyn FnOnce() -> Result<()> + Send>>,
    ) -> Self {
        Self { rx, close_fn }
    }
}

#[async_trait]
impl StreamResult for ChannelStreamResult {
    async fn next(&mut self) -> Result<Option<Value>> {
        match self.rx.recv().await {
            Some(Ok(v)) => Ok(Some(v)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    async fn close(&mut self) -> Result<()> {
        if let Some(close_fn) = self.close_fn.take() {
            close_fn()?;
        }
        Ok(())
    }
}

/// StreamResult backed by an in-memory vector (useful for adapting eager responses).
pub struct VecStreamResult {
    items: Vec<Value>,
    index: usize,
    close_fn: Option<Box<dyn FnOnce() -> Result<()> + Send>>,
}

impl VecStreamResult {
    pub fn new(
        items: Vec<Value>,
        close_fn: Option<Box<dyn FnOnce() -> Result<()> + Send>>,
    ) -> Self {
        Self {
            items,
            index: 0,
            close_fn,
        }
    }
}

#[async_trait]
impl StreamResult for VecStreamResult {
    async fn next(&mut self) -> Result<Option<Value>> {
        if self.index >= self.items.len() {
            return Ok(None);
        }
        let item = self.items[self.index].clone();
        self.index += 1;
        Ok(Some(item))
    }

    async fn close(&mut self) -> Result<()> {
        if let Some(close_fn) = self.close_fn.take() {
            close_fn()?;
        }
        Ok(())
    }
}

/// Helper to box a channel-backed stream result.
pub fn boxed_channel_stream(
    rx: mpsc::Receiver<Result<Value>>,
    close_fn: Option<Box<dyn FnOnce() -> Result<()> + Send>>,
) -> Box<dyn StreamResult> {
    Box::new(ChannelStreamResult::new(rx, close_fn))
}

/// Helper to box a vector-backed stream result.
pub fn boxed_vec_stream(items: Vec<Value>) -> Box<dyn StreamResult> {
    Box::new(VecStreamResult::new(items, None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn vec_stream_iterates_and_closes() {
        let closed = Arc::new(AtomicBool::new(false));
        let closed_clone = closed.clone();
        let mut stream = VecStreamResult::new(
            vec![json!(1), json!({"two": 2})],
            Some(Box::new(move || {
                closed_clone.store(true, Ordering::SeqCst);
                Ok(())
            })),
        );

        assert_eq!(stream.next().await.unwrap(), Some(json!(1)));
        assert_eq!(stream.next().await.unwrap(), Some(json!({"two": 2})));
        assert_eq!(stream.next().await.unwrap(), None);
        stream.close().await.unwrap();
        assert!(closed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn channel_stream_propagates_error() {
        let (tx, rx) = mpsc::channel(2);
        tx.send(Ok(json!("ok"))).await.unwrap();
        tx.send(Err(anyhow::anyhow!("boom"))).await.unwrap();
        drop(tx);

        let mut stream = ChannelStreamResult::new(rx, None);
        assert_eq!(stream.next().await.unwrap(), Some(json!("ok")));
        let err = stream.next().await.unwrap_err();
        assert!(format!("{err}").contains("boom"));
        assert_eq!(stream.next().await.unwrap(), None);
    }
}
