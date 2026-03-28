use tokio::io::{AsyncBufRead, AsyncBufReadExt};
use wrangle_core::ParseError;

pub const MAX_JSON_LINE_BYTES: usize = 256 * 1024;

pub struct JsonLineParser<R> {
    reader: R,
    buffer: Vec<u8>,
}

impl<R: AsyncBufRead + Unpin> JsonLineParser<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: Vec::with_capacity(4096),
        }
    }

    pub async fn next_event(&mut self) -> Option<Result<serde_json::Value, ParseError>> {
        loop {
            self.buffer.clear();
            match self.reader.read_until(b'\n', &mut self.buffer).await {
                Ok(0) => return None,
                Ok(n) => {
                    if n > MAX_JSON_LINE_BYTES || self.buffer.len() > MAX_JSON_LINE_BYTES {
                        return Some(Err(ParseError::MessageTooLarge {
                            max_bytes: MAX_JSON_LINE_BYTES,
                        }));
                    }

                    let line = String::from_utf8_lossy(&self.buffer);
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    if !line.starts_with('{') && !line.starts_with('[') {
                        continue;
                    }

                    match serde_json::from_str::<serde_json::Value>(line) {
                        Ok(value) => return Some(Ok(value)),
                        Err(_) => continue,
                    }
                }
                Err(err) => return Some(Err(ParseError::Io(err.to_string()))),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn parses_json_lines() {
        let input = "{\"ok\":true}\nnope\n{\"done\":true}\n";
        let mut parser = JsonLineParser::new(BufReader::new(input.as_bytes()));
        assert_eq!(
            parser.next_event().await.unwrap().unwrap()["ok"],
            serde_json::json!(true)
        );
        assert_eq!(
            parser.next_event().await.unwrap().unwrap()["done"],
            serde_json::json!(true)
        );
        assert!(parser.next_event().await.is_none());
    }
}
