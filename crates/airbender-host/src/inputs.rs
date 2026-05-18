use crate::error::Result;
use airbender_codec::{AirbenderCodec, AirbenderCodecV1};
use airbender_core::wire::frame_words_from_bytes;
use std::fmt::Write as _;
use std::path::Path;

/// Typed input builder for host-to-guest communication.
#[derive(Clone, Debug, Default)]
pub struct Inputs {
    words: Vec<u32>,
}

impl Inputs {
    pub fn new() -> Self {
        Self { words: Vec::new() }
    }

    /// Serialize and append a typed input value.
    pub fn push<T: serde::Serialize>(&mut self, value: &T) -> Result<()> {
        let bytes = AirbenderCodecV1::encode(value)?;
        self.push_bytes(&bytes)?;
        Ok(())
    }

    /// Append raw bytes as a framed input payload.
    pub fn push_bytes(&mut self, bytes: &[u8]) -> Result<()> {
        let words = frame_words_from_bytes(bytes)?;
        self.words.extend(words);
        Ok(())
    }

    /// Access the framed input words.
    pub fn words(&self) -> &[u32] {
        &self.words
    }

    /// Write input words as CLI-compatible hex (`8` hex chars per `u32`).
    pub fn write_hex_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let mut hex = String::new();
        for word in &self.words {
            writeln!(&mut hex, "{word:08x}").expect("writing to string cannot fail");
        }
        std::fs::write(path, hex)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Inputs;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn writes_hex_file_with_one_word_per_line() {
        let mut inputs = Inputs::new();
        inputs.push_bytes(&[0x29]).expect("frame input bytes");

        let file_path = test_file_path("inputs-hex");
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).expect("create test parent directory");
        }

        inputs
            .write_hex_file(&file_path)
            .expect("write input hex file");

        let written = fs::read_to_string(&file_path).expect("read written input hex file");
        assert_eq!(written, "00000001\n00000029\n");

        fs::remove_file(&file_path).expect("remove input hex file");
    }

    #[test]
    fn serializes_u32_as_fixed_int_payload() {
        let mut inputs = Inputs::new();
        inputs.push(&10u32).expect("frame input value");

        // V1 codec uses fixed-int encoding: u32 is always 4 bytes (LE).
        // Frame: length word (4) + one data word (0x0000000a in LE).
        assert_eq!(inputs.words(), &[4, 0x0000000a]);
    }

    fn test_file_path(prefix: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tmp")
            .join(format!("{prefix}-{timestamp}-{}", std::process::id()))
            .join("input.hex")
    }
}
