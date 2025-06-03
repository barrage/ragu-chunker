use super::DocumentType;
use crate::{core::model::image::Image, err, error::ChonkitError, map_err};
use serde::{Deserialize, Serialize};
use validify::{schema_err, schema_validation, Validate, ValidationErrors};

pub mod docx;
pub mod excel;
pub mod pdf;
pub mod text;

pub async fn parse(
    config: ParseConfig,
    ext: DocumentType,
    input: &[u8],
) -> Result<ParseOutput, ChonkitError> {
    map_err!(config.validate());

    let ParseConfig {
        mode,
        include_images,
    } = config;

    match mode {
        ParseMode::String(config) => {
            if let DocumentType::Pdf = ext {
                let (text, images) = pdf::parse_to_string(&config, input, include_images).await?;

                if text.trim().is_empty() && images.is_empty() {
                    return err!(InvalidFile, "Parsing resulted in empty output");
                }

                return Ok(ParseOutput::String { text, images });
            }

            let out = match ext {
                DocumentType::Text(_) => text::parse(&config, input)?,
                DocumentType::Docx => docx::parse(&config, input)?,
                DocumentType::Excel => excel::parse(&config, input)?,
                _ => unreachable!(),
            };

            if out.trim().is_empty() {
                return err!(InvalidFile, "Parsing resulted in empty output");
            }

            Ok(ParseOutput::String {
                text: out,
                images: vec![],
            })
        }
        ParseMode::Section(config) => match ext {
            DocumentType::Pdf => {
                let out = pdf::parse_to_sections(&config, input, include_images).await?;

                if out.is_empty() {
                    return err!(InvalidFile, "Parsing resulted in empty output");
                }

                Ok(ParseOutput::Sections(out))
            }
            _ => err!(
                InvalidParameter,
                "Sectioned parsing not supported for document type '{ext}'"
            ),
        },
    }
}

/// Parsing configuration for a document.
#[derive(Debug, Default, Clone, Serialize, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ParseConfig {
    #[validate]
    pub mode: ParseMode,
    pub include_images: bool,
}

/// Parsing mode determines the output of the parser.
#[derive(Debug, Clone, Serialize, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum ParseMode {
    String(#[validate] StringParseConfig),
    Section(#[validate] SectionParseConfig),
}

impl Default for ParseMode {
    fn default() -> Self {
        ParseMode::String(StringParseConfig::default())
    }
}

/// Note: PartialEq implementation checks text only.
#[derive(Debug)]
pub enum ParseOutput {
    String {
        /// The parsed text.
        text: String,
        /// Base64 data uris for all images found in the document.
        images: Vec<Image>,
    },
    Sections(Vec<DocumentSection>),
}

impl PartialEq for ParseOutput {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ParseOutput::String { text: a, .. }, ParseOutput::String { text: b, .. }) => a == b,
            (ParseOutput::Sections(a), ParseOutput::Sections(b)) => a == b,
            _ => false,
        }
    }
}

/// Generic parsing configuration for documents based on text elements.
///
/// A text element is parser specific, it could be PDF pages,
/// DOCX paragraphs, CSV rows, etc.
#[derive(Debug, Default, Clone, Serialize, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
#[validate(Self::validate_schema)]
pub struct StringParseConfig {
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

impl StringParseConfig {
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
#[derive(Debug, Default, PartialEq)]
pub struct DocumentSection {
    pub pages: Vec<DocumentPage>,
}

/// A document page that has been parsed with a parser using [SectionParseConfig].
#[derive(Debug, PartialEq)]
pub struct DocumentPage {
    /// The text contents of the page.
    pub content: String,

    /// The page number.
    pub number: usize,

    pub images: Vec<Image>,
}
