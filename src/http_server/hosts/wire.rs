//! Agent link framing: `u32` big-endian length, one kind byte, then the payload.
//!
//! Control messages are JSON objects tagged by `type`. Snapshot and activity
//! payloads are the agent's SSE JSON, so the hub relays them without a schema copy.

use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub(super) const PROTOCOL: u32 = 1;
/// The largest frame, including its kind byte.
pub(super) const MAX_FRAME_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FrameKind {
    Control,
    Snapshot,
    Activity,
}
impl FrameKind {
    fn code(self) -> u8 {
        match self {
            Self::Control => 1,
            Self::Snapshot => 2,
            Self::Activity => 3,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum Control {
    /// The agent's first message. The token is compared in constant time.
    Hello {
        protocol: u32,
        host: String,
        machine_id: String,
        version: String,
        kernel: Option<String>,
        token: Option<String>,
    },
    Welcome {
        protocol: u32,
        version: String,
    },
    /// The hub refuses the agent and closes the link.
    Reject {
        reason: String,
    },
    /// Whether any browser watches this host; agents collect only while active and
    /// answer each activation with a full snapshot.
    Watch {
        active: bool,
    },
    /// The hub lost the agent's delta baseline and asks for a full snapshot.
    Resync,
    Ping,
    Pong,
}

pub(super) async fn read_frame(
    reader: &mut (impl AsyncRead + Unpin),
) -> Result<(FrameKind, Vec<u8>)> {
    let length = reader.read_u32().await? as usize;
    ensure!(
        (1..=MAX_FRAME_BYTES).contains(&length),
        "agent frame length {length} is out of range"
    );
    let kind = match reader.read_u8().await? {
        1 => FrameKind::Control,
        2 => FrameKind::Snapshot,
        3 => FrameKind::Activity,
        code => bail!("unknown agent frame kind {code}"),
    };
    let mut payload = vec![0; length - 1];
    reader.read_exact(&mut payload).await?;
    Ok((kind, payload))
}

pub(super) async fn write_frame(
    writer: &mut (impl AsyncWrite + Unpin),
    kind: FrameKind,
    payload: &[u8],
) -> Result<()> {
    ensure!(payload.len() < MAX_FRAME_BYTES, "agent frame is too large");
    writer.write_u32(payload.len() as u32 + 1).await?;
    writer.write_u8(kind.code()).await?;
    writer.write_all(payload).await?;
    writer.flush().await?;
    Ok(())
}

pub(super) async fn write_control(
    writer: &mut (impl AsyncWrite + Unpin),
    control: &Control,
) -> Result<()> {
    write_frame(writer, FrameKind::Control, &serde_json::to_vec(control)?).await
}

pub(super) fn parse_control(payload: &[u8]) -> Result<Control> {
    Ok(serde_json::from_slice(payload)?)
}

/// Compares secrets without an early exit on the first differing byte.
pub(super) fn same_secret(expected: &str, actual: &str) -> bool {
    let (expected, actual) = (expected.as_bytes(), actual.as_bytes());
    expected.len() == actual.len()
        && expected
            .iter()
            .zip(actual)
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn frames_round_trip_and_reject_bad_lengths_and_kinds() {
        let mut buffer = Vec::new();
        write_control(&mut buffer, &Control::Watch { active: true })
            .await
            .unwrap();
        write_frame(&mut buffer, FrameKind::Snapshot, b"{}")
            .await
            .unwrap();
        let mut reader = buffer.as_slice();
        let (kind, payload) = read_frame(&mut reader).await.unwrap();
        assert_eq!(kind, FrameKind::Control);
        assert_eq!(
            parse_control(&payload).unwrap(),
            Control::Watch { active: true }
        );
        assert_eq!(
            String::from_utf8(payload).unwrap(),
            r#"{"type":"watch","active":true}"#
        );
        assert_eq!(
            read_frame(&mut reader).await.unwrap(),
            (FrameKind::Snapshot, b"{}".to_vec())
        );
        assert!(read_frame(&mut reader).await.is_err(), "end of stream");
        for frame in [
            [0u8, 0, 0, 0].to_vec(),
            ((MAX_FRAME_BYTES + 1) as u32).to_be_bytes().to_vec(),
            [0, 0, 0, 1, 9].to_vec(),
            [0, 0, 0, 5, 2, b'{'].to_vec(),
        ] {
            assert!(read_frame(&mut frame.as_slice()).await.is_err());
        }
        assert!(parse_control(br#"{"type":"unknown"}"#).is_err());
    }

    #[test]
    fn secrets_compare_whole_values() {
        assert!(same_secret("token", "token"));
        assert!(!same_secret("token", "tokem"));
        assert!(!same_secret("token", "token2"));
        assert!(!same_secret("", "x"));
    }
}
