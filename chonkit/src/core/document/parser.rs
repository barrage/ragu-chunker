use super::DocumentType;
use crate::{err, error::ChonkitError};
use serde::{Deserialize, Serialize};
use validify::{schema_err, schema_validation, Validate, ValidationErrors};

pub mod docx;
pub mod excel;
pub mod pdf;
pub mod text;

#[derive(Debug)]
pub struct Parser<C = GenericParseConfig>(pub C);

impl<C> Parser<C> {
    pub fn new(config: C) -> Self {
        Self(config)
    }
}

impl Parser<GenericParseConfig> {
    pub fn parse(&self, ext: DocumentType, input: &[u8]) -> Result<String, ChonkitError> {
        let out = match ext {
            DocumentType::Text(_) => text::parse(input, &self.0),
            DocumentType::Docx => docx::parse(input, &self.0),
            DocumentType::Pdf => pdf::parse(input, &self.0),
            DocumentType::Excel => excel::parse(input, &self.0),
        }?;

        let GenericParseConfig {
            start, end, range, ..
        } = self.0;

        if out.trim().is_empty() {
            tracing::error!("Parsing resulted in empty output. Config: {:?}", self.0);

            return crate::err!(
                ParseConfig,
                "empty output (start: {start} | end: {end} | range: {range})",
            );
        }

        Ok(out)
    }
}

impl Parser<SectionParseConfig> {
    pub fn parse(
        &self,
        ext: DocumentType,
        input: &[u8],
    ) -> Result<Vec<DocumentSection>, ChonkitError> {
        match ext {
            DocumentType::Pdf => pdf::parse_paginated(input, &self.0),
            _ => err!(
                InvalidParameter,
                "cannot parse {ext} with pagination parser"
            ),
        }
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new(GenericParseConfig::default())
    }
}

/// Enumerations of all possible parsing configurations chonkit supports.
#[derive(Debug, Clone, Serialize, Deserialize, Validate, utoipa::ToSchema)]
#[serde(untagged)]
pub enum ParseConfig {
    Generic(#[validate] GenericParseConfig),
    Sectioned(#[validate] SectionParseConfig),
}

#[derive(Debug, Serialize, Deserialize, Validate, utoipa::ToSchema)]
#[serde(untagged)]
pub enum ParseOutput {
    Generic(String),
    Sectioned(Vec<DocumentSection>),
}

/// Generic parsing configuration for documents based on text elements.
///
/// A text element is parser specific, it could be PDF pages,
/// DOCX paragraphs, CSV rows, etc.
#[derive(Debug, Default, Clone, Serialize, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
#[validate(Self::validate_schema)]
pub struct GenericParseConfig {
    /// Skip the first amount of text elements.
    pub start: usize,

    /// Skip the last amount of text elements.
    pub end: usize,

    /// If true, parsers should treat the the (start)[Self::start]
    /// and (end)[Self::end] parameters as a range instead of just
    /// skipping the elements.
    pub range: bool,

    /// Exclude specific lines matching any of the patterns provided here from the output.
    pub filters: Vec<String>,
}

impl GenericParseConfig {
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

/// Pagination based parser.
///
/// Applicable to documents with pagination:
///
/// - DOCX
/// - PDF
#[derive(Debug, Clone, Serialize, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SectionParseConfig {
    /// A list of document sections (pages) to capture in the final output.
    #[validate]
    pub sections: Vec<PageRange>,

    /// Exclude lines matching any of the provided filters.
    pub filters: Vec<String>,
}

/// Represents a range of pages in a document to capture in the final output.
///
/// Both `start` and `end` are inclusive. Given `start == end`, a single page will be captured.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
#[validate(Self::schema_validation)]
pub struct PageRange {
    #[validate(range(min = 1.))]
    pub start: usize,
    pub end: usize,
}

impl PageRange {
    #[schema_validation]
    fn schema_validation(&self) -> Result<(), ValidationErrors> {
        if self.start > self.end {
            schema_err!(
                "start>end",
                "section end must be greater than or equal to start"
            );
        }
    }
}

/// A document section that has been parsed with a parser using [SectionParseConfig].
#[derive(Debug, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DocumentSection {
    pub pages: Vec<DocumentPage>,
}

/// A document page that has been parsed with a parser using [SectionParseConfig].
#[derive(Debug, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DocumentPage {
    /// The text contents of the page.
    pub content: String,

    /// The page number.
    pub number: usize,
}
