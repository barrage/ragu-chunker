use super::{
    cursor::{byte_count, Cursor, CursorRev, DEFAULT_SKIP_B, DEFAULT_SKIP_F},
    ChunkerError,
};

const SNAPPING_WINDOW_DEFAULT_SIZE: usize = 1000;
const SNAPPING_WINDOW_DEFAULT_OVERLAP: usize = 5;

/// Heuristic chunker for texts intended for humans, e.g. documentation, books, blogs, etc.
///
/// A sliding window that is aware of sentence stops,
///
/// It will attempt to chunk the content according to `size`. Keep in mind it cannot
/// be exact and the chunks will probably be larger, because of the way it searches
/// for delimiters.
///
/// The chunker can also be configured to skip common patterns including the fullstop
/// such as abbreviations (e.g., i.e., etc.) and urls.
///
/// The default delimiter is `'.'`.
/// The default `size` and `overlap` are 1000 and 10.
/// The default forward skips are [SnappingWindow::DEFAULT_SKIP_F].
/// The default backward skips are [SnappingWindow::DEFAULT_SKIP_B].
///
/// Keep in mind the configuration for this chunker is different; The `size` will
/// represent the amount of bytes in the chunk and the `overlap` will represent
/// how many back/forward sentences will be included.
///
/// If the input has a lot of unicode with characters more than 1 byte, a larger `size` is
/// recommended.
#[derive(Debug, Clone)]
pub struct SnappingWindow {
    /// Here `size` represents the amount of bytes in the base chunk
    /// while `overlap` will represent the amount of leading/trailing sentences.
    pub size: usize,

    pub overlap: usize,

    /// The delimiter to use to split sentences. At time of writing the most common one is ".".
    pub delimiter: char,

    /// Whenever a delimiter is found, the chunker will look ahead for these sequences
    /// and will skip the delimiter if found, treating it as a regular char.
    ///
    /// Useful for common abbreviations and urls.
    pub skip_forward: Vec<String>,

    /// Whenever a delimiter is found, the chunker will look back for these sequences
    /// and will skip the delimiter if found, treating it as a regular char.
    ///
    /// Useful for common abbreviations and urls.
    pub skip_back: Vec<String>,
}

impl Default for SnappingWindow {
    fn default() -> Self {
        Self::new(
            SNAPPING_WINDOW_DEFAULT_SIZE,
            SNAPPING_WINDOW_DEFAULT_OVERLAP,
        )
        .expect("overlap is greater than size")
        .skip_forward(DEFAULT_SKIP_F.iter().map(|e| e.to_string()).collect())
        .skip_back(DEFAULT_SKIP_B.iter().map(|e| e.to_string()).collect())
    }
}

impl SnappingWindow {
    pub fn new(size: usize, overlap: usize) -> Result<Self, ChunkerError> {
        if overlap > size {
            return Err(ChunkerError::Config(
                "overlap must be less than size".to_string(),
            ));
        }
        Ok(Self {
            size,
            overlap,
            delimiter: '.',
            skip_forward: DEFAULT_SKIP_F.iter().map(|e| e.to_string()).collect(),
            skip_back: DEFAULT_SKIP_B.iter().map(|e| e.to_string()).collect(),
        })
    }

    pub fn delimiter(mut self, delimiter: char) -> Self {
        self.delimiter = delimiter;
        self
    }

    /// Set the forward skips.
    pub fn skip_forward(mut self, skip_forward: Vec<String>) -> Self {
        self.skip_forward = skip_forward;
        self
    }

    /// Set the backward skips.
    pub fn skip_back(mut self, skip_back: Vec<String>) -> Self {
        self.skip_back = skip_back;
        self
    }

    /// Extend the forward and backward skips.
    pub fn extend_skips(mut self, skip_forward: Vec<String>, skip_back: Vec<String>) -> Self {
        self.skip_forward.extend(skip_forward);
        self.skip_back.extend(skip_back);
        self
    }
}

impl SnappingWindow {
    pub fn chunk(&self, input: &str) -> Result<Vec<String>, ChunkerError> {
        if input.trim().is_empty() {
            return Ok(vec![]);
        }

        let Self {
            size,
            overlap,
            delimiter,
            skip_forward,
            skip_back,
        } = self;

        let total_bytes = byte_count(input);
        let mut chunks = vec![];

        let mut chars = input.chars().peekable();

        // The current byte offset
        let mut current_offset = 0;

        let mut chunk = String::with_capacity(*size + *overlap * 2);
        let mut chunk_byte_size = 0;

        'outer: while let Some(char) = chars.next() {
            current_offset += char.len_utf8();

            if char != *delimiter {
                chunk.push(char);
                chunk_byte_size += char.len_utf8();
                continue;
            }

            // If we haven't reached the size yet, push the delimiter

            if chunk_byte_size < *size {
                chunk.push(char);
                chunk_byte_size += char.len_utf8();
                continue;
            }

            // Maximum chunk size reached, check skips

            for skip in skip_back {
                if chunk.ends_with(skip) {
                    chunk.push(char);
                    chunk_byte_size += char.len_utf8();

                    // Special case if skip is at the end of the input
                    if current_offset == total_bytes {
                        let prev = &input[..current_offset - chunk_byte_size];
                        let prev =
                            previous_chunk(prev, *overlap, *delimiter, skip_forward, skip_back);
                        chunks.push(format!("{prev}{chunk}"));
                        break 'outer;
                    }

                    continue 'outer;
                }
            }

            // Skip any delimiters not followed by a space
            // so as to skip the next check
            if let Some(ch) = chars.peek() {
                if *ch != ' ' {
                    chunk.push(char);
                    chunk_byte_size += char.len_utf8();
                    continue;
                }
            }

            for skip in skip_forward {
                if input[current_offset..].starts_with(skip) {
                    chunk.push(char);
                    chunk_byte_size += char.len_utf8();
                    continue 'outer;
                }
            }

            // Add the delimiter to the chunk
            chunk.push(char);
            chunk_byte_size += char.len_utf8();

            let prev = &input[..current_offset - chunk_byte_size];

            // Don't add anything if the prev is empty, we are at the start
            // and the next chunk will account for that
            if prev.is_empty() {
                chunk.clear();
                chunk_byte_size = 0;
                continue;
            }

            let next = &input[current_offset..];

            let prev = previous_chunk(prev, *overlap, *delimiter, skip_forward, skip_back);
            let (next, next_offset) =
                next_chunk(next, *overlap, *delimiter, skip_forward, skip_back);

            // No point in going further if the lookahead has reached the end
            if current_offset + next_offset == total_bytes - 1 {
                chunks.push(format!("{prev}{chunk}{next}"));
                break;
            }

            chunks.push(format!("{prev}{chunk}{next}"));
            chunk.clear();
            chunk_byte_size = 0;
        }

        Ok(chunks)
    }
}

#[inline(always)]
fn previous_chunk<'a>(
    input: &'a str,
    overlap: usize,
    delim: char,
    skip_forward: &[String],
    skip_back: &[String],
) -> &'a str {
    let mut p_cursor = CursorRev::new(input, delim);
    for _ in 0..overlap {
        p_cursor.advance();
        while p_cursor.advance_if_peek(skip_forward, skip_back) {
            p_cursor.advance();
        }
    }
    p_cursor.get_slice()
}

#[inline(always)]
fn next_chunk<'a>(
    input: &'a str,
    overlap: usize,
    delim: char,
    skip_forward: &[String],
    skip_back: &[String],
) -> (&'a str, usize) {
    let mut n_cursor = Cursor::new(input, delim);
    for _ in 0..overlap {
        n_cursor.advance();
        while n_cursor.advance_if_peek(skip_forward, skip_back) {
            n_cursor.advance();
        }
    }
    (n_cursor.get_slice(), n_cursor.byte_offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_size() {
        let ch = 'Ü';
        let mut bytes = [0, 0];
        assert_eq!(2, ch.encode_utf8(&mut bytes).len());
    }

    #[test]
    fn constructor() {
        // For lifetime sanity checks
        let skip_f = vec![String::from("foo"), String::from("bar")];
        let skip_b = vec![String::from("foo"), String::from("bar")];
        let size = 1;
        let overlap = 1;
        let delimiter = '!';

        let chunker = SnappingWindow::new(size, overlap)
            .unwrap()
            .delimiter(delimiter)
            .skip_forward(skip_f.clone())
            .skip_back(skip_b.clone());

        assert_eq!(delimiter, chunker.delimiter);
        assert_eq!(size, chunker.size);
        assert_eq!(overlap, chunker.overlap);
        assert_eq!(skip_f, chunker.skip_forward);
        assert_eq!(skip_b, chunker.skip_back);
    }

    #[tokio::test]
    async fn snapping_works() {
        let input =
            "I have a sentence. It is not very long. Here is another. Long schlong ding dong.";
        let chunker = SnappingWindow::new(1, 1).unwrap();
        let expected = [
            "I have a sentence. It is not very long. Here is another.",
            " It is not very long. Here is another. Long schlong ding dong.",
        ];

        let chunks = chunker.chunk(input.trim()).unwrap();
        dbg!(&chunks);
        assert_eq!(2, chunks.len());

        for (chunk, test) in chunks.into_iter().zip(expected.into_iter()) {
            assert_eq!(test, chunk);
        }
    }

    #[tokio::test]
    async fn snapping_skips_back() {
        let input =
            "I have a sentence. It contains letters, words, etc. and it contains more. The most important of which is foobar., because it must be skipped.";
        let chunker = SnappingWindow::new(1, 1)
            .unwrap()
            .skip_back(vec!["etc".to_string(), "foobar".to_string()]);
        let expected = [input];

        let chunks = chunker.chunk(input.trim()).unwrap();
        dbg!(&chunks);
        assert_eq!(1, chunks.len());

        for (chunk, test) in chunks.into_iter().zip(expected.into_iter()) {
            assert_eq!(test, chunk);
        }
    }

    #[tokio::test]
    async fn snapping_skips_forward() {
        let input =
            "Go to sentences.org for more words. 50% off on words with >4 syllables. Leverage agile frameworks to provide robust high level overview at agile.com.";

        let chunker = SnappingWindow::new(1, 1)
            .unwrap()
            .skip_forward(vec!["com".to_string(), "org".to_string()]);
        let expected = [input];

        let chunks = chunker.chunk(input.trim()).unwrap();
        assert_eq!(1, chunks.len());

        for (chunk, test) in chunks.into_iter().zip(expected.into_iter()) {
            assert_eq!(test, chunk);
        }
    }

    #[tokio::test]
    async fn snapping_skips_common_abbreviations() {
        let input =
            "Words are hard. There are many words in existence, e.g. this, that, etc..., quite a few, as you can see. My opinion, available at nobodycares.com, is that words should convey meaning. Not everyone agrees however, which is why they leverage agile frameworks to provide robust synopses for high level overviews. The lucidity of meaning is, in fact, obscured and ambiguous, therefore the interpretation, i.e. the conveying of units of meaning is less than optimal. Jebem ti boga.";

        let chunker = SnappingWindow::new(1, 1).unwrap();

        let expected = [
            "Words are hard. There are many words in existence, e.g. this, that, etc..., quite a few, as you can see. My opinion, available at nobodycares.com, is that words should convey meaning.",
            " There are many words in existence, e.g. this, that, etc..., quite a few, as you can see. My opinion, available at nobodycares.com, is that words should convey meaning. Not everyone agrees however, which is why they leverage agile frameworks to provide robust synopses for high level overviews.",
            " My opinion, available at nobodycares.com, is that words should convey meaning. Not everyone agrees however, which is why they leverage agile frameworks to provide robust synopses for high level overviews. The lucidity of meaning is, in fact, obscured and ambiguous, therefore the interpretation, i.e. the conveying of units of meaning is less than optimal.",
            " Not everyone agrees however, which is why they leverage agile frameworks to provide robust synopses for high level overviews. The lucidity of meaning is, in fact, obscured and ambiguous, therefore the interpretation, i.e. the conveying of units of meaning is less than optimal. Jebem ti boga.",
        ];

        let chunks = chunker.chunk(input.trim()).unwrap();
        assert_eq!(4, chunks.len());

        for (chunk, test) in chunks.into_iter().zip(expected.into_iter()) {
            assert_eq!(test, chunk);
        }
    }

    #[tokio::test]
    async fn snapping_table_of_contents() {
        let input =
            "Table of contents:\n1 Super cool stuff\n1.1 Some chonkers in rust\n1.2 Some data for your LLM\n1.3 ??? \n1.4 Profit \n1.4.1 Lambo\nHope you liked the table of contents. See more at content.co.com.";

        let chunker = SnappingWindow::new(1, 1)
            .unwrap()
            .skip_forward(vec![
                "0".to_string(),
                "1".to_string(),
                "2".to_string(),
                "3".to_string(),
                "4".to_string(),
                "co".to_string(),
                "com".to_string(),
            ])
            .skip_back(vec!["com".to_string()]);

        let expected = [input];

        let chunks = chunker.chunk(input.trim()).unwrap();
        assert_eq!(1, chunks.len());

        for (chunk, test) in chunks.into_iter().zip(expected.into_iter()) {
            assert_eq!(test, chunk);
        }
    }

    #[tokio::test]
    async fn snapping_window_empty() {
        let chunker = SnappingWindow::new(1, 1).unwrap();
        let chunks = chunker.chunk("").unwrap();
        assert!(chunks.is_empty());
    }
}
