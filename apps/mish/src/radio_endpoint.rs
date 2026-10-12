//! Shared transport opening for explicit commissioning and verified runtime reconnects.
use anyhow::{Context, Result, bail};
use std::{net::SocketAddr, path::Path, time::Duration};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_serial::SerialPortBuilderExt;
pub trait RadioStream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> RadioStream for T {}
pub fn validate(endpoint: &str) -> Result<()> {
    if let Some(address) = endpoint.strip_prefix("tcp://") {
        let address: SocketAddr = address
            .parse()
            .context("TCP radio endpoint requires a literal IP address and port")?;
        if address.port() == 0 || address.ip().is_unspecified() || address.ip().is_multicast() {
            bail!("invalid TCP radio endpoint");
        }
    } else {
        let path = Path::new(endpoint);
        if path.parent() != Some(Path::new("/dev/serial/by-id")) || path.file_name().is_none() {
            bail!("radio device must use /dev/serial/by-id/");
        }
    }
    Ok(())
}
pub async fn open(endpoint: &str) -> Result<Box<dyn RadioStream>> {
    validate(endpoint)?;
    if let Some(address) = endpoint.strip_prefix("tcp://") {
        let stream = tokio::time::timeout(
            Duration::from_secs(10),
            tokio::net::TcpStream::connect(address),
        )
        .await
        .context("TCP radio connection timed out")??;
        stream.set_nodelay(true)?;
        Ok(Box::new(stream))
    } else {
        Ok(Box::new(
            tokio_serial::new(endpoint, 115200).open_native_async()?,
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn tcp_stream_opens_only_explicit_valid_endpoints() {
        for invalid in [
            "tcp://example.org:4403",
            "tcp://0.0.0.0:4403",
            "tcp://127.0.0.1:0",
            "/dev/ttyUSB0",
        ] {
            assert!(validate(invalid).is_err());
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("tcp://{}", listener.local_addr().unwrap());
        let _stream = open(&endpoint).await.unwrap();
        listener.accept().await.unwrap();
    }
}
