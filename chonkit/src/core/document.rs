use sha2::{Digest, Sha256};

use crate::{err, error::ChonkitError};

/// Parsing implementations for various file types.
pub mod parser;

/// File system storage implementations.
pub mod store;

pub struct Docx<'a>(pub &'a [u8]);
pub struct Pdf<'a>(pub &'a [u8]);
pub struct Text<'a>(pub &'a [u8]);
pub struct Excel<'a>(pub &'a [u8]);

/// All possible file types chonkit can process.
#[derive(Debug, Clone, Copy)]
pub enum DocumentType {
    /// Encapsulates any files that can be read as strings.
    /// Does not necessarily have to be `.txt`, could be `.json`, `.csv`, etc.
    Text(TextDocumentType),

    /// Microschlong steaming pile of garbage document.
    Docx,

    /// PDF document.
    Pdf,

    Excel,
}

#[derive(Debug, Clone, Copy)]
pub enum TextDocumentType {
    Md,
    Xml,
    Json,
    Csv,
    Txt,
}

impl DocumentType {
    pub fn try_from_file_name(name: &str) -> Result<Self, ChonkitError> {
        let Some((_, ext)) = name.rsplit_once('.') else {
            return err!(UnsupportedFileType, "{name} - missing extension");
        };
        Self::try_from(ext)
    }
}

impl std::fmt::Display for DocumentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DocumentType::Text(ty) => match ty {
                TextDocumentType::Md => write!(f, "md"),
                TextDocumentType::Xml => write!(f, "xml"),
                TextDocumentType::Json => write!(f, "json"),
                TextDocumentType::Csv => write!(f, "csv"),
                TextDocumentType::Txt => write!(f, "txt"),
            },
            DocumentType::Docx => write!(f, "docx"),
            DocumentType::Pdf => write!(f, "pdf"),
            DocumentType::Excel => write!(f, "xlsx"),
        }
    }
}

impl TryFrom<&str> for DocumentType {
    type Error = ChonkitError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "xlsx" | "application/vnd.google-apps.spreadsheet" => Ok(Self::Excel),
            "md" => Ok(Self::Text(TextDocumentType::Md)),
            "xml" => Ok(Self::Text(TextDocumentType::Xml)),
            "json" | "application/json" => Ok(Self::Text(TextDocumentType::Json)),
            "csv" => Ok(Self::Text(TextDocumentType::Csv)),
            "txt" | "text/plain" => Ok(Self::Text(TextDocumentType::Txt)),
            "pdf" | "application/pdf" => Ok(Self::Pdf),
            "docx"
            | "application/vnd.google-apps.document"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => {
                Ok(Self::Docx)
            }
            _ => err!(UnsupportedFileType, "{value}"),
        }
    }
}

impl TryFrom<String> for DocumentType {
    type Error = ChonkitError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

/// Return a SHA256 hash of the input.
///
/// * `input`: Input bytes.
pub fn sha256(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    Digest::update(&mut hasher, input);
    let out = hasher.finalize();
    hex::encode(out)
}
