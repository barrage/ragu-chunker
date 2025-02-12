use crate::error::ChonkitError;
use serde::{Deserialize, Serialize};
use validify::{schema_err, schema_validation, Validate, ValidationErrors};

use super::{DocumentType, Docx, Excel, Pdf, Text};

pub mod docx;
pub mod excel;
pub mod pdf;
pub mod text;

#[derive(Debug)]
pub struct Parser<C = ParseConfig>(pub C);

impl Parser {
    pub fn new(config: ParseConfig) -> Self {
        Self(config)
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new(ParseConfig::default())
    }
}

/// Generic parsing configuration for documents.
/// A text element is parser specific, it could be PDF pages,
/// DOCX paragraphs, CSV rows, etc.
#[derive(Debug, Default, Clone, Serialize, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
#[validate(Self::validate_schema)]
pub struct ParseConfig {
    /// Skip the first amount of text elements.
    pub start: usize,

    /// Skip the last amount of text elements.
    pub end: usize,

    /// If true, parsers should treat the the (start)[Self::start]
    /// and (end)[Self::end] parameters as a range instead of just
    /// skipping the elements.
    pub range: bool,

    /// Filter specific patterns in text elements. Parser specific.
    pub filters: Vec<String>,
}

impl ParseConfig {
    pub fn new(start: usize, end: usize) -> Self {
        Self {
            start,
            end,
            ..Default::default()
        }
    }

    /// Set the parser to use a range of elements instead of just skipping.
    pub fn use_range(mut self) -> Self {
        self.range = true;
        self
    }

    /// Add a filter to the parser.
    ///
    /// * `re`: The expression to match for.
    pub fn with_filter(mut self, re: &str) -> Self {
        self.filters.push(re.to_string());
        self
    }

    #[schema_validation]
    fn validate_schema(&self) -> Result<(), ValidationErrors> {
        if self.range && self.end <= self.start {
            schema_err!(
                "range=true;start>=end",
                "end must be greater than start when using range"
            );
        }
        if self.range && self.start == 0 {
            schema_err!("range=true;start=0", "start cannot be 0 when using range");
        }
    }
}

pub trait Parse<T> {
    fn parse(&self, input: T) -> Result<String, ChonkitError>;
}

impl Parse<Docx<'_>> for Parser {
    fn parse(&self, input: Docx<'_>) -> Result<String, ChonkitError> {
        docx::parse(input.0, &self.0)
    }
}

impl Parse<Pdf<'_>> for Parser {
    fn parse(&self, input: Pdf<'_>) -> Result<String, ChonkitError> {
        let out = pdf::parse(input.0, &self.0)?;

        Ok(out)
    }
}

impl Parse<Excel<'_>> for Parser {
    fn parse(&self, input: Excel<'_>) -> Result<String, ChonkitError> {
        excel::parse(input.0, &self.0)
    }
}

impl Parse<Text<'_>> for Parser {
    fn parse(&self, input: Text<'_>) -> Result<String, ChonkitError> {
        text::parse(input.0, &self.0)
    }
}

impl Parser {
    pub fn parse_bytes(&self, ext: DocumentType, input: &[u8]) -> Result<String, ChonkitError> {
        let out = match ext {
            DocumentType::Text(_) => Parse::parse(self, Text(input)),
            DocumentType::Docx => Parse::parse(self, Docx(input)),
            DocumentType::Pdf => Parse::parse(self, Pdf(input)),
            DocumentType::Excel => Parse::parse(self, Excel(input)),
        }?;

        let ParseConfig {
            start, end, range, ..
        } = self.0;

        if out.is_empty() {
            tracing::error!("Parsing resulted in empty output. Config: {:?}", self.0);

            return crate::err!(
                ParseConfig,
                "empty output (start: {start} | end: {end} | range: {range})",
            );
        }

        Ok(out)
    }
}
