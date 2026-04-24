use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::{self, StreamExt, TryStreamExt};
use futures::Stream;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio_util_hack::ReaderStream;
use url::Url;

mod tokio_util_hack {
    //! Tiny reader→stream adapter so we don't drag in tokio-util just for this.
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use bytes::Bytes;
    use futures::Stream;
    use tokio::io::{AsyncRead, ReadBuf};

    pub(super) struct ReaderStream<R> {
        reader: Pin<Box<R>>,
        buf: Vec<u8>,
    }

    impl<R: AsyncRead + Send + 'static> ReaderStream<R> {
        pub(super) fn new(reader: R) -> Self {
            Self { reader: Box::pin(reader), buf: vec![0u8; 64 * 1024] }
        }
    }

    impl<R: AsyncRead + Send + 'static> Stream for ReaderStream<R> {
        type Item = Result<Bytes, std::io::Error>;
        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            let me = &mut *self;
            let mut rb = ReadBuf::new(&mut me.buf);
            match me.reader.as_mut().poll_read(cx, &mut rb) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Err(e)) => Poll::Ready(Some(Err(e))),
                Poll::Ready(Ok(())) => {
                    let n = rb.filled().len();
                    if n == 0 {
                        Poll::Ready(None)
                    } else {
                        Poll::Ready(Some(Ok(Bytes::copy_from_slice(&me.buf[..n]))))
                    }
                }
            }
        }
    }
}

use crate::config::MediaConfig;
use crate::error::{MediaError, MediaResult};
use crate::store::{ByteStream, MediaRef, MediaStore};

pub async fn connect(cfg: &MediaConfig) -> MediaResult<Box<dyn MediaStore>> {
    let MediaConfig::Local { path, base_url } = cfg else {
        unreachable!();
    };
    fs::create_dir_all(path).await?;
    Ok(Box::new(LocalStore { root: path.clone(), base_url: base_url.clone() }))
}

pub struct LocalStore {
    root: PathBuf,
    base_url: Option<String>,
}

impl std::fmt::Debug for LocalStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalStore").field("root", &self.root).finish()
    }
}

impl LocalStore {
    fn path_for(&self, key: &str) -> MediaResult<PathBuf> {
        if key.contains("..") || key.starts_with('/') {
            return Err(MediaError::Backend(format!("invalid key `{key}`")));
        }
        Ok(self.root.join(key))
    }
}

#[async_trait]
impl MediaStore for LocalStore {
    async fn put(
        &self,
        key: &str,
        mut body: ByteStream,
        mime: &str,
        _size: u64,
    ) -> MediaResult<MediaRef> {
        let p = self.path_for(key)?;
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).await?;
        }
        let mut f = fs::File::create(&p).await?;
        let mut written: u64 = 0;
        while let Some(chunk) = body.next().await {
            let chunk = chunk?;
            written += chunk.len() as u64;
            f.write_all(&chunk).await?;
        }
        f.flush().await?;

        let url = match &self.base_url {
            Some(b) => Some(Url::parse(&format!("{}/{}", b.trim_end_matches('/'), key)).ok())
                .flatten(),
            None => None,
        };
        Ok(MediaRef { key: key.to_string(), size: written, mime: mime.to_string(), url })
    }

    async fn get(&self, key: &str) -> MediaResult<ByteStream> {
        let p = self.path_for(key)?;
        let file = fs::File::open(&p).await.map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => MediaError::NotFound,
            _ => MediaError::Io(e),
        })?;
        let stream: ByteStream = Box::pin(ReaderStream::new(file));
        Ok(stream)
    }

    async fn delete(&self, key: &str) -> MediaResult<()> {
        let p = self.path_for(key)?;
        match fs::remove_file(p).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    async fn exists(&self, key: &str) -> MediaResult<bool> {
        Ok(self.path_for(key)?.exists())
    }

    async fn presign_get(&self, key: &str, _ttl: Duration) -> MediaResult<Url> {
        let base = self
            .base_url
            .as_deref()
            .ok_or_else(|| MediaError::Backend("local store has no base_url configured".into()))?;
        Url::parse(&format!("{}/{}", base.trim_end_matches('/'), key))
            .map_err(|e| MediaError::Backend(e.to_string()))
    }
}

/// Helper: wrap any `impl Stream<Bytes>` as a boxed `ByteStream`.
pub fn into_byte_stream<S>(s: S) -> ByteStream
where
    S: Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
{
    Box::pin(s)
}

/// Helper: one-shot byte stream from a `Vec<u8>`.
pub fn once_bytes(v: Vec<u8>) -> ByteStream {
    let s = stream::iter(std::iter::once(Ok(Bytes::from(v))));
    Box::pin(s.into_stream())
}
