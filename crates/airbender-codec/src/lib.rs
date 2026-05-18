#![doc = include_str!("../README.md")]
#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt;

#[cfg(test)]
extern crate std;

/// Stable codec version for host/guest communication.
pub const AIRBENDER_CODEC_V0: u32 = 0;

/// Codec version with fixed-int encoding (no varints).
pub const AIRBENDER_CODEC_V1: u32 = 1;

/// A stable, versioned serializer used by Airbender host and guest programs.
pub trait AirbenderCodec {
    /// Version identifier baked into manifests and tooling.
    const VERSION: u32;

    /// Serialize a value into a byte payload.
    fn encode<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, CodecError>;

    /// Deserialize a value from a byte payload.
    fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, CodecError>;
}

/// Initial codec based on `bincode` v2 with varint encoding.
pub struct AirbenderCodecV0;

impl AirbenderCodec for AirbenderCodecV0 {
    const VERSION: u32 = AIRBENDER_CODEC_V0;

    fn encode<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, CodecError> {
        bincode::serde::encode_to_vec(value, bincode::config::standard())
            .map_err(CodecError::Encode)
    }

    fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, CodecError> {
        let (decoded, read_len) =
            bincode::serde::decode_from_slice(bytes, bincode::config::standard())
                .map_err(CodecError::Decode)?;
        if read_len != bytes.len() {
            return Err(CodecError::TrailingBytes {
                expected: bytes.len(),
                read: read_len,
            });
        }
        Ok(decoded)
    }
}

/// Fixed-int codec: same as V0 but uses fixed-width integer encoding.
/// Every u32 is 4 bytes, every u64 is 8 bytes — no varint branches.
/// Better performance for crypto-heavy workloads (U256, Bytes32, hashes).
pub struct AirbenderCodecV1;

impl AirbenderCodec for AirbenderCodecV1 {
    const VERSION: u32 = AIRBENDER_CODEC_V1;

    fn encode<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, CodecError> {
        bincode::serde::encode_to_vec(value, bincode::config::standard().with_fixed_int_encoding())
            .map_err(CodecError::Encode)
    }

    fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, CodecError> {
        let (decoded, read_len) = bincode::serde::decode_from_slice(
            bytes,
            bincode::config::standard().with_fixed_int_encoding(),
        )
        .map_err(CodecError::Decode)?;
        if read_len != bytes.len() {
            return Err(CodecError::TrailingBytes {
                expected: bytes.len(),
                read: read_len,
            });
        }
        Ok(decoded)
    }
}

#[derive(Debug)]
pub enum CodecError {
    Encode(bincode::error::EncodeError),
    Decode(bincode::error::DecodeError),
    TrailingBytes { expected: usize, read: usize },
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodecError::Encode(_) => f.write_str("failed to encode value"),
            CodecError::Decode(_) => f.write_str("failed to decode value"),
            CodecError::TrailingBytes { expected, read } => {
                write!(f, "decoded {read} bytes but expected {expected}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
    struct Sample {
        value: u32,
        payload: alloc::vec::Vec<u8>,
    }

    #[test]
    fn codec_v0_roundtrip() {
        let sample = Sample {
            value: 42,
            payload: vec![1u8, 2, 3, 4, 5],
        };
        let encoded = AirbenderCodecV0::encode(&sample).expect("encode");
        let decoded: Sample = AirbenderCodecV0::decode(&encoded).expect("decode");
        assert_eq!(decoded, sample);
    }

    #[test]
    fn codec_v1_roundtrip() {
        let sample = Sample {
            value: 42,
            payload: vec![1u8, 2, 3, 4, 5],
        };
        let encoded = AirbenderCodecV1::encode(&sample).expect("encode");
        let decoded: Sample = AirbenderCodecV1::decode(&encoded).expect("decode");
        assert_eq!(decoded, sample);
    }

    #[test]
    fn v1_uses_fixed_int_encoding() {
        // u32 value 42 should be exactly 4 bytes in V1 (fixed), but 1 byte in V0 (varint)
        let encoded_v0 = AirbenderCodecV0::encode(&42u32).expect("v0 encode");
        let encoded_v1 = AirbenderCodecV1::encode(&42u32).expect("v1 encode");
        assert_eq!(encoded_v0.len(), 1); // varint: 42 < 251, so 1 byte
        assert_eq!(encoded_v1.len(), 4); // fixed: always 4 bytes for u32
    }
}
