//! Guest input helpers backed by the Airbender codec.

use crate::transport::Transport;
use airbender_codec::{AirbenderCodec, AirbenderCodecV0, CodecError};
use airbender_core::wire::read_framed_bytes_with;
use core::fmt;

/// Errors that can occur when decoding inputs on the guest.
#[derive(Debug)]
pub enum GuestError {
    Codec(CodecError),
    UnsupportedTarget,
}

impl From<CodecError> for GuestError {
    fn from(err: CodecError) -> Self {
        GuestError::Codec(err)
    }
}

impl fmt::Display for GuestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GuestError::Codec(err) => write!(f, "{err}"),
            GuestError::UnsupportedTarget => {
                f.write_str("csr transport is only available on riscv32")
            }
        }
    }
}

/// Read a single value from the CSR-based transport.
pub fn read<T: serde::de::DeserializeOwned>() -> Result<T, GuestError> {
    #[cfg(target_arch = "riscv32")]
    {
        let mut transport = crate::transport::CsrTransport;
        read_with(&mut transport)
    }
    #[cfg(not(target_arch = "riscv32"))]
    {
        Err(GuestError::UnsupportedTarget)
    }
}

/// Read a single value using an explicit transport.
pub fn read_with<T: serde::de::DeserializeOwned>(
    transport: &mut impl Transport,
) -> Result<T, GuestError> {
    let bytes = read_framed_bytes_with(|| transport.read_word());
    AirbenderCodecV0::decode(&bytes).map_err(GuestError::Codec)
}

/// Read a single value using V1 codec (fixed-int encoding, no varints).
pub fn read_v1_with<T: serde::de::DeserializeOwned>(
    transport: &mut impl Transport,
) -> Result<T, GuestError> {
    use airbender_codec::AirbenderCodecV1;
    let bytes = read_framed_bytes_with(|| transport.read_word());
    AirbenderCodecV1::decode(&bytes).map_err(GuestError::Codec)
}

/// Read a fixed-size value without heap allocation.
///
/// The caller provides `ENCODED_SIZE` — the exact number of payload bytes
/// (excluding the length prefix word). The value is decoded from a stack
/// buffer, avoiding `Vec` allocation entirely.
///
/// # Panics
/// Panics if the framed length word doesn't match `ENCODED_SIZE`.
pub fn read_fixed_with<T: serde::de::DeserializeOwned, const ENCODED_SIZE: usize>(
    transport: &mut impl Transport,
) -> Result<T, GuestError> {
    use airbender_codec::AirbenderCodecV1;

    let len = transport.read_word() as usize;
    assert!(
        len == ENCODED_SIZE,
        "read_fixed: expected {ENCODED_SIZE} bytes, got {len}"
    );

    let words_needed = (ENCODED_SIZE + 3) / 4;
    let mut buf = [0u8; ENCODED_SIZE];
    let mut offset = 0;
    for _ in 0..words_needed {
        let word_bytes = transport.read_word().to_le_bytes();
        let to_copy = (ENCODED_SIZE - offset).min(4);
        buf[offset..offset + to_copy].copy_from_slice(&word_bytes[..to_copy]);
        offset += to_copy;
    }

    AirbenderCodecV1::decode(&buf).map_err(GuestError::Codec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockTransport;
    use airbender_core::wire::frame_words_from_bytes;
    use alloc::vec;

    #[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
    struct Payload {
        counter: u32,
        bytes: alloc::vec::Vec<u8>,
    }

    #[test]
    fn reads_value_from_transport() {
        let payload = Payload {
            counter: 7,
            bytes: vec![10u8, 20, 30],
        };
        let encoded = AirbenderCodecV0::encode(&payload).expect("encode");
        let words = frame_words_from_bytes(&encoded).expect("frame words");
        let mut transport = MockTransport::new(words);
        let decoded: Payload = read_with(&mut transport).expect("read");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn reads_v1_value_from_transport() {
        use airbender_codec::AirbenderCodecV1;
        let value = 0xDEADBEEFu32;
        let encoded = AirbenderCodecV1::encode(&value).expect("encode");
        let words = frame_words_from_bytes(&encoded).expect("frame words");
        let mut transport = MockTransport::new(words);
        let decoded: u32 = read_v1_with(&mut transport).expect("read");
        assert_eq!(decoded, value);
    }

    #[test]
    fn reads_fixed_value_without_alloc() {
        use airbender_codec::AirbenderCodecV1;
        // [u64; 4] with fixed-int encoding = 32 bytes exactly
        let value: [u64; 4] = [1, 2, 3, 4];
        let encoded = AirbenderCodecV1::encode(&value).expect("encode");
        assert_eq!(encoded.len(), 32);
        let words = frame_words_from_bytes(&encoded).expect("frame words");
        let mut transport = MockTransport::new(words);
        let decoded: [u64; 4] = read_fixed_with::<_, 32>(&mut transport).expect("read");
        assert_eq!(decoded, value);
    }
}
