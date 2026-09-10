use bytes::Buf;
use bytes::Bytes;
use futures_util::Stream;
use indicatif::{ProgressBar, ProgressStyle};
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{self, AsyncRead};

/// 将 bytes_stream 包装为带进度更新的 AsyncRead
pub struct ProgressStream<S: Stream<Item = Result<Bytes, reqwest::Error>>> {
  inner: S,
  progress: ProgressBar,
  buf: Bytes,
}

impl<S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin>
  ProgressStream<S>
{
  pub fn new(stream: S, total: u64) -> Self {
    let progress = ProgressBar::new(total);
    progress.set_style(
      ProgressStyle::default_bar()
        .template("downloading [{bar:40}] [{percent:.2}%] {total_bytes} {eta}")
        .expect("invalid template")
        .progress_chars("=> "),
    );
    Self {
      inner: stream,
      progress,
      buf: Bytes::new(),
    }
  }

  pub fn finish(self) {
    self.progress.finish();
  }
}

impl<S: Stream<Item = Result<Bytes, reqwest::Error>> + Unpin> AsyncRead
  for ProgressStream<S>
{
  fn poll_read(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut io::ReadBuf<'_>,
  ) -> Poll<io::Result<()>> {
    // 如果缓冲区还有剩余数据，直接填充
    if !self.buf.is_empty() {
      let to_copy = self.buf.len().min(buf.remaining());
      buf.put_slice(&self.buf[..to_copy]);
      self.buf.advance(to_copy);
      return Poll::Ready(Ok(()));
    }

    // 否则从流中拉取下一个 chunk
    match Pin::new(&mut self.inner).poll_next(cx) {
      Poll::Ready(Some(Ok(chunk))) => {
        self.progress.inc(chunk.len() as u64);
        let to_copy = chunk.len().min(buf.remaining());
        buf.put_slice(&chunk[..to_copy]);
        if to_copy < chunk.len() {
          self.buf = chunk.slice(to_copy..);
        }
        Poll::Ready(Ok(()))
      }
      Poll::Ready(Some(Err(e))) => Poll::Ready(Err(io::Error::other(e))),
      Poll::Ready(None) => Poll::Ready(Ok(())), // EOF
      Poll::Pending => Poll::Pending,
    }
  }
}
