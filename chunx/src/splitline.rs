/// The line chunker matches patterns of text on lines to determine chunk boundaries.
/// The chunker is line based, which makes it useful for processing any documents that can
/// be chunked based on lines.
///
/// Examples of such documents are CSVs, markdown, and any document
/// formed with some kind of distinguishable header (e.g. PDFs with section headers in formats of
/// X. Header, X.Y. Subheader).
///
/// To use the chunker as a section based chunker, set the size to `usize::MAX` and specify the
/// header patterns. If size is set to max and no patterns are specified, the chunker will
/// return a single chunk for the entire document.
#[derive(Debug)]
pub struct Splitline {
    /// The maximum number of lines to include in each chunk.
    /// Headers are excluded from the count.
    size: usize,

    /// The line patterns that determine chunk boundaries. The very first line of the text is always used as a
    /// header, regardless of patterns.
    patterns: Vec<regex::Regex>,

    /// If `true`, prepends the last header found to each subsequent chunk until another header is
    /// found or the end of file is reached. Useful for CSVs.
    prepend_latest_header: bool,
}

impl Splitline {
    pub fn new(size: usize, patterns: Vec<regex::Regex>, include_last_header: bool) -> Self {
        Self {
            size,
            patterns,
            prepend_latest_header: include_last_header,
        }
    }

    pub fn chunk(&self, input: &str) -> Vec<String> {
        let mut result = vec![];

        let mut lines = input.lines();

        let Some(mut header) = lines.next() else {
            return vec![];
        };

        if input.trim().len() == header.len() {
            return vec![input.to_string()];
        }

        let mut buf = String::from(header);
        let mut amount = 0;

        for line in lines {
            buf.push('\n');

            if amount == self.size {
                result.push(buf);
                buf = if self.prepend_latest_header {
                    let mut header = String::from(header);
                    header.push('\n');
                    header
                } else {
                    String::new()
                };
                amount = 0;
            }

            if self.patterns.iter().any(|pattern| pattern.is_match(line)) {
                if amount > 0 {
                    result.push(buf);
                }
                buf = String::from(line);
                amount = 0;
                header = line;
                continue;
            }

            buf.push_str(line);
            amount += 1;
        }

        if amount > 0 {
            if input.ends_with('\n') {
                buf.push('\n');
            }
            result.push(buf);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use regex::Regex;

    use super::*;

    #[test]
    fn splitline_single_chunk() {
        let input = "NAME,AGE,GENDER\nJohn,32,M\nJane,28,F\nBob,45,M\nAlice,23,F";

        let chunker = Splitline::new(1000, vec![], false);

        let expected = ["NAME,AGE,GENDER\nJohn,32,M\nJane,28,F\nBob,45,M\nAlice,23,F"];

        let chunks = chunker.chunk(input);

        assert_eq!(1, chunks.len());

        for (chunk, test) in chunks.into_iter().zip(expected.into_iter()) {
            assert_eq!(test, chunk);
        }
    }

    #[test]
    fn splitline_size() {
        let input = "NAME,AGE,GENDER\nJohn,32,M\nJane,28,F\nBob,45,M\nAlice,23,F";

        let chunker = Splitline::new(2, vec![], false);

        let expected = [
            "NAME,AGE,GENDER\nJohn,32,M\nJane,28,F\n",
            "Bob,45,M\nAlice,23,F",
        ];

        let chunks = chunker.chunk(input);

        assert_eq!(2, chunks.len());

        for (chunk, test) in chunks.into_iter().zip(expected.into_iter()) {
            assert_eq!(test, chunk);
        }
    }

    #[test]
    fn splitline_patterns() {
        let input = "NAME,AGE,GENDER\nJohn,32,M\nJane,28,F\nBob,45,M\nAlice,23,F\nFOO,BAR,QUX,QAZ\n1,2,3,4\n5,6,7,8";

        let chunker = Splitline::new(
            10,
            vec![regex::Regex::new("FOO,BAR,QUX,QAZ").unwrap()],
            false,
        );

        let expected = [
            "NAME,AGE,GENDER\nJohn,32,M\nJane,28,F\nBob,45,M\nAlice,23,F\n",
            "FOO,BAR,QUX,QAZ\n1,2,3,4\n5,6,7,8",
        ];

        let chunks = chunker.chunk(input);

        assert_eq!(2, chunks.len());

        for (chunk, test) in chunks.into_iter().zip(expected.into_iter()) {
            assert_eq!(test, chunk);
        }
    }

    #[test]
    fn splitline_patterns_prepend() {
        let input = "NAME,AGE,GENDER\nJohn,32,M\nJane,28,F\nBob,45,M\nAlice,23,F\nFOO,BAR,QUX,QAZ\n1,2,3,4\n5,6,7,8\n9,10,11,12\n13,14,15,16";

        let chunker = Splitline::new(2, vec![regex::Regex::new("FOO,BAR,QUX,QAZ").unwrap()], true);

        let expected = [
            "NAME,AGE,GENDER\nJohn,32,M\nJane,28,F\n",
            "NAME,AGE,GENDER\nBob,45,M\nAlice,23,F\n",
            "FOO,BAR,QUX,QAZ\n1,2,3,4\n5,6,7,8\n",
            "FOO,BAR,QUX,QAZ\n9,10,11,12\n13,14,15,16",
        ];

        let chunks = chunker.chunk(input);

        assert_eq!(4, chunks.len());

        for (chunk, test) in chunks.into_iter().zip(expected.into_iter()) {
            assert_eq!(test, chunk);
        }
    }

    #[test]
    fn splitline_patterns_prepend_newline() {
        let input = "NAME,AGE,GENDER\nJohn,32,M\nJane,28,F\nBob,45,M\nAlice,23,F\nFOO,BAR,QUX,QAZ\n1,2,3,4\n5,6,7,8\n9,10,11,12\n13,14,15,16\n";

        let chunker = Splitline::new(2, vec![regex::Regex::new("FOO,BAR,QUX,QAZ").unwrap()], true);

        let expected = [
            "NAME,AGE,GENDER\nJohn,32,M\nJane,28,F\n",
            "NAME,AGE,GENDER\nBob,45,M\nAlice,23,F\n",
            "FOO,BAR,QUX,QAZ\n1,2,3,4\n5,6,7,8\n",
            "FOO,BAR,QUX,QAZ\n9,10,11,12\n13,14,15,16\n",
        ];

        let chunks = chunker.chunk(input);

        assert_eq!(4, chunks.len());

        for (chunk, test) in chunks.into_iter().zip(expected.into_iter()) {
            assert_eq!(test, chunk);
        }
    }

    #[test]
    fn splitline_empty() {
        let chunker = Splitline::new(2, vec![], false);
        let chunks = chunker.chunk("");
        assert!(chunks.is_empty());
    }

    #[test]
    fn splitline_header() {
        let chunker = Splitline::new(2, vec![], false);
        let expected = ["NAME,AGE,GENDER"];
        let chunks = chunker.chunk("NAME,AGE,GENDER");
        assert_eq!(1, chunks.len());
        for (chunk, test) in chunks.into_iter().zip(expected.into_iter()) {
            assert_eq!(test, chunk);
        }
    }

    #[test]
    fn splitline_header_newline() {
        let chunker = Splitline::new(2, vec![], false);
        let expected = ["NAME,AGE,GENDER\n"];
        let chunks = chunker.chunk("NAME,AGE,GENDER\n");
        assert_eq!(1, chunks.len());
        for (chunk, test) in chunks.into_iter().zip(expected.into_iter()) {
            assert_eq!(test, chunk);
        }
    }

    #[test]
    fn splitline_section_split() {
        let chunker = Splitline::new(200, vec![Regex::new(r#"^\s*\d\. .+$"#).unwrap()], false);
        let input = r#"
        1. A
          1.1 A1
          1.2 A2
          1.2.1 A2.1
        2. B
          2.1 B1
          2.2 B2
          2.2.1 B2.1
        3. C
          3.1 C1
          3.2 C2
          3.3 C3
        "#;

        let expected = [
            r#"
        1. A
          1.1 A1
          1.2 A2
          1.2.1 A2.1
        "#,
            r#"
        2. B
          2.1 B1
          2.2 B2
          2.2.1 B2.1
        "#,
            r#"
        3. C
          3.1 C1
          3.2 C2
          3.3 C3
            "#,
        ];

        let chunks = chunker.chunk(input);

        assert_eq!(3, chunks.len());

        for (chunk, test) in chunks.into_iter().zip(expected.into_iter()) {
            assert_eq!(test.trim(), chunk.trim());
        }
    }
}
