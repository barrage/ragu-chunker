use super::ParseConfig;
use crate::{error::ChonkitError, map_err};
use calamine::{Reader, Xlsx};
use serde::{Deserialize, Serialize};
use std::fmt::Write;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExcelParser {
    config: ParseConfig,
}

impl ExcelParser {
    pub fn new(config: ParseConfig) -> Self {
        Self { config }
    }

    pub fn parse(&self, input: &[u8]) -> Result<String, ChonkitError> {
        let cursor = std::io::Cursor::new(input);
        let mut reader = map_err!(Xlsx::new(cursor));
        let sheets = reader.worksheets();

        let mut csv = String::new();

        for (_, sheet) in sheets {
            for row in sheet.rows() {
                let csv_row = row
                    .iter()
                    .map(|r| r.to_string())
                    .collect::<Vec<String>>()
                    .join(",");

                let _ = writeln!(&mut csv, "{csv_row}",);
            }
        }

        Ok(csv)
    }
}
