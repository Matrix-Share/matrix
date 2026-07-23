//! Relay wire protocol + length-prefixed CBOR framing.
//!
//! The relay is deliberately **zero-knowledge**: it forwards opaque `data`
//! blobs (which are end-to-end-encrypted Lifeline frames) between connected
//! nodes and never inspects them. It is the one optional centralized component
//! — the "internet gateway fabric" — and it can read/forge nothing (FR-38,
//! gateway doc §5).

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Max on-wire frame size (defensive bound against a bogus length prefix).
pub const MAX_FRAME: usize = 4 * 1024 * 1024;

/// Node → relay messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Up {
    /// Forward `data` to peer `to` (unicast) or to everyone else (`None`).
    Send {
        to: Option<u64>,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },
}

/// Relay → node messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Down {
    /// Sent once on connect: your assigned id and the currently-connected peers.
    Welcome { your_id: u64, peers: Vec<u64> },
    /// A peer connected.
    Join { id: u64 },
    /// A peer disconnected.
    Leave { id: u64 },
    /// An opaque frame from peer `from`.
    Frame {
        from: u64,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },
}

/// Errors from framing.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("frame too large: {0} > {MAX_FRAME}")]
    TooLarge(usize),
    #[error("cbor: {0}")]
    Cbor(String),
}

fn to_cbor<T: Serialize>(v: &T) -> Result<Vec<u8>, FrameError> {
    let mut buf = Vec::new();
    ciborium::into_writer(v, &mut buf).map_err(|e| FrameError::Cbor(e.to_string()))?;
    Ok(buf)
}

fn from_cbor<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, FrameError> {
    ciborium::from_reader(bytes).map_err(|e| FrameError::Cbor(e.to_string()))
}

/// Write a length-prefixed CBOR message.
pub async fn write_msg<W, T>(w: &mut W, msg: &T) -> Result<(), FrameError>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let body = to_cbor(msg)?;
    if body.len() > MAX_FRAME {
        return Err(FrameError::TooLarge(body.len()));
    }
    w.write_all(&(body.len() as u32).to_be_bytes()).await?;
    w.write_all(&body).await?;
    w.flush().await?;
    Ok(())
}

/// Read a length-prefixed CBOR message. Returns `Ok(None)` on clean EOF.
pub async fn read_msg<R, T>(r: &mut R) -> Result<Option<T>, FrameError>
where
    R: AsyncRead + Unpin,
    T: for<'de> Deserialize<'de>,
{
    let mut len_buf = [0u8; 4];
    match r.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME {
        return Err(FrameError::TooLarge(len));
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).await?;
    Ok(Some(from_cbor(&buf)?))
}

// --- Blocking (std::io) variants, used by the node's relay client ---

/// Write a length-prefixed CBOR message over a blocking writer.
pub fn write_msg_sync<W: std::io::Write, T: Serialize>(
    w: &mut W,
    msg: &T,
) -> Result<(), FrameError> {
    let body = to_cbor(msg)?;
    if body.len() > MAX_FRAME {
        return Err(FrameError::TooLarge(body.len()));
    }
    w.write_all(&(body.len() as u32).to_be_bytes())?;
    w.write_all(&body)?;
    w.flush()?;
    Ok(())
}

/// Read a length-prefixed CBOR message over a blocking reader. `Ok(None)` on EOF.
pub fn read_msg_sync<R: std::io::Read, T: for<'de> Deserialize<'de>>(
    r: &mut R,
) -> Result<Option<T>, FrameError> {
    let mut len_buf = [0u8; 4];
    match std::io::Read::read_exact(r, &mut len_buf) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME {
        return Err(FrameError::TooLarge(len));
    }
    let mut buf = vec![0u8; len];
    std::io::Read::read_exact(r, &mut buf)?;
    Ok(Some(from_cbor(&buf)?))
}
