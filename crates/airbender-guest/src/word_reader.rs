use crate::transport::Transport;
use core::mem::MaybeUninit;
use wincode::io::{ReadResult, Reader};

/// A wincode [`Reader`] backed by a word-based [`Transport`].
///
/// Reads u32 words from the transport and serves them as bytes.
/// Maintains a 4-byte internal buffer for partial word reads.
pub struct WordReader<'a, T: Transport> {
    transport: &'a mut T,
    buf: [u8; 4],
    pos: usize, // next byte to read from buf
    len: usize, // valid bytes in buf
}

impl<'a, T: Transport> WordReader<'a, T> {
    pub fn new(transport: &'a mut T) -> Self {
        Self {
            transport,
            buf: [0; 4],
            pos: 0,
            len: 0,
        }
    }

    #[inline(always)]
    fn refill(&mut self) {
        let word = self.transport.read_word();
        self.buf = word.to_le_bytes();
        self.pos = 0;
        self.len = 4;
    }
}

impl<'a, T: Transport> Reader<'a> for WordReader<'a, T> {
    fn copy_into_slice(&mut self, dst: &mut [MaybeUninit<u8>]) -> ReadResult<()> {
        let mut written = 0;
        let total = dst.len();

        // Drain any leftover bytes from the current word
        while written < total && self.pos < self.len {
            dst[written] = MaybeUninit::new(self.buf[self.pos]);
            self.pos += 1;
            written += 1;
        }

        // Read full words directly
        while total - written >= 4 {
            let word = self.transport.read_word();
            let bytes = word.to_le_bytes();
            dst[written] = MaybeUninit::new(bytes[0]);
            dst[written + 1] = MaybeUninit::new(bytes[1]);
            dst[written + 2] = MaybeUninit::new(bytes[2]);
            dst[written + 3] = MaybeUninit::new(bytes[3]);
            written += 4;
        }

        // Read a partial word for remaining bytes
        if written < total {
            self.refill();
            while written < total {
                dst[written] = MaybeUninit::new(self.buf[self.pos]);
                self.pos += 1;
                written += 1;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::MockTransport;
    use alloc::vec;
    use alloc::vec::Vec;

    #[test]
    fn reads_exact_words() {
        let mut transport = MockTransport::new(vec![0x04030201, 0x08070605]);
        let mut reader = WordReader::new(&mut transport);
        let result: [u8; 8] = reader.take_array().unwrap();
        assert_eq!(result, [1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn reads_partial_word() {
        let mut transport = MockTransport::new(vec![0x04030201]);
        let mut reader = WordReader::new(&mut transport);
        let result: [u8; 3] = reader.take_array().unwrap();
        assert_eq!(result, [1, 2, 3]);
    }

    #[test]
    fn reads_across_word_boundary() {
        let mut transport = MockTransport::new(vec![0x04030201, 0x08070605]);
        let mut reader = WordReader::new(&mut transport);
        let a: [u8; 3] = reader.take_array().unwrap();
        let b: [u8; 3] = reader.take_array().unwrap();
        assert_eq!(a, [1, 2, 3]);
        assert_eq!(b, [4, 5, 6]);
    }

    #[test]
    fn reads_u64_le() {
        // 0x0807060504030201 as two LE u32 words
        let mut transport = MockTransport::new(vec![0x04030201, 0x08070605]);
        let mut reader = WordReader::new(&mut transport);
        let bytes: [u8; 8] = reader.take_array().unwrap();
        let value = u64::from_le_bytes(bytes);
        assert_eq!(value, 0x0807060504030201);
    }

    #[derive(Debug, PartialEq, wincode::SchemaRead, wincode::SchemaWrite)]
    struct DivRemResponse {
        quotient: [u64; 4],
    }

    #[test]
    fn wincode_deserialize_struct() {
        // DivRemResponse { quotient: [1, 2, 3, 4] } as LE u32 words
        let words: Vec<u32> = vec![1, 0, 2, 0, 3, 0, 4, 0];
        let mut transport = MockTransport::new(words);
        let reader = WordReader::new(&mut transport);
        let result: DivRemResponse = wincode::deserialize_from(reader).unwrap();
        assert_eq!(
            result,
            DivRemResponse {
                quotient: [1, 2, 3, 4]
            }
        );
    }

    #[test]
    fn wincode_serialize_roundtrip() {
        let original = DivRemResponse {
            quotient: [0xDEAD, 0xBEEF, 0xCAFE, 0xBABE],
        };
        let bytes: Vec<u8> = wincode::serialize(&original).unwrap();
        // Feed the bytes as u32 LE words
        let mut words: Vec<u32> = Vec::new();
        for chunk in bytes.chunks(4) {
            let mut buf = [0u8; 4];
            let chunk_slice: &[u8] = chunk;
            buf[..chunk_slice.len()].copy_from_slice(chunk_slice);
            words.push(u32::from_le_bytes(buf));
        }
        let mut transport = MockTransport::new(words);
        let reader = WordReader::new(&mut transport);
        let result: DivRemResponse = wincode::deserialize_from(reader).unwrap();
        assert_eq!(result, original);
    }

    #[derive(Debug, PartialEq, wincode::SchemaRead, wincode::SchemaWrite)]
    struct ModexpResponse {
        quotient: Vec<u64>,
        remainder: Vec<u64>,
    }

    #[test]
    fn wincode_deserialize_vec_struct() {
        let original = ModexpResponse {
            quotient: vec![0xAA, 0xBB],
            remainder: vec![0xCC],
        };
        let bytes: Vec<u8> = wincode::serialize(&original).unwrap();
        let mut words: Vec<u32> = Vec::new();
        for chunk in bytes.chunks(4) {
            let mut buf = [0u8; 4];
            let chunk_slice: &[u8] = chunk;
            buf[..chunk_slice.len()].copy_from_slice(chunk_slice);
            words.push(u32::from_le_bytes(buf));
        }
        let mut transport = MockTransport::new(words);
        let reader = WordReader::new(&mut transport);
        let result: ModexpResponse = wincode::deserialize_from(reader).unwrap();
        assert_eq!(result, original);
    }

    #[test]
    fn wincode_deserialize_u64_array() {
        // [u64; 4] = [1, 2, 3, 4] serialized as LE bytes
        let words: Vec<u32> = vec![
            1, 0, // u64 1
            2, 0, // u64 2
            3, 0, // u64 3
            4, 0, // u64 4
        ];
        let mut transport = MockTransport::new(words);
        let reader = WordReader::new(&mut transport);
        let result: [u64; 4] = wincode::deserialize_from(reader).unwrap();
        assert_eq!(result, [1u64, 2, 3, 4]);
    }
}
