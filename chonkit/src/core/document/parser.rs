use crate::error::ChonkitError;
use serde::{Deserialize, Serialize};
use validify::{schema_err, schema_validation, Validate, ValidationErrors};

use super::{Docx, Excel, Pdf, Text};

pub mod docx;
pub mod excel;
pub mod pdf;
pub mod text;

#[derive(Debug, Default)]
pub struct Parser<C = ParseConfig>(C);

impl Parser {
    pub fn new(config: ParseConfig) -> Self {
        Self(config)
    }
}

pub trait Parse<T> {
    fn parse(&self, input: T) -> Result<String, ChonkitError>;
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

impl Parse<Docx<'_>> for Parser {
    fn parse(&self, input: Docx<'_>) -> Result<String, ChonkitError> {
        docx::parse(input.0, &self.0)
    }
}

impl Parse<Pdf<'_>> for Parser {
    fn parse(&self, input: Pdf<'_>) -> Result<String, ChonkitError> {
        pdf::parse(input.0, &self.0)
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

#[macro_export]
macro_rules! parse {
    ($parser:expr, $ext:expr, $content:expr) => {
        match $ext {
            $crate::core::document::DocumentType::Text(_) => {
                $crate::core::document::parser::Parse::parse(
                    $parser,
                    $crate::core::document::Text($content),
                )
            }
            $crate::core::document::DocumentType::Docx => {
                $crate::core::document::parser::Parse::parse(
                    $parser,
                    $crate::core::document::Docx($content),
                )
            }
            $crate::core::document::DocumentType::Pdf => {
                $crate::core::document::parser::Parse::parse(
                    $parser,
                    $crate::core::document::Pdf($content),
                )
            }
            $crate::core::document::DocumentType::Excel => {
                $crate::core::document::parser::Parse::parse(
                    $parser,
                    $crate::core::document::Excel($content),
                )
            }
        }
    };
}
