use super::ParseConfig;
use crate::{err, error::ChonkitError, map_err};
use calamine::{Reader, Xlsx};
use std::fmt::Write;

pub fn parse(input: &[u8], config: &ParseConfig) -> Result<String, ChonkitError> {
    let cursor = std::io::Cursor::new(input);
    let mut reader = map_err!(Xlsx::new(cursor));
    let sheets = reader.worksheets();

    let mut result = vec![];

    for (sheet_name, sheet) in sheets {
        if !config.filters.is_empty() && !config.filters.contains(&sheet_name) {
            continue;
        }

        let mut csv = String::new();

        let mut rows = sheet.rows();

        let Some(header) = rows.next() else {
            return err!(InvalidFile, "Excel file has no rows");
        };

        let header = header
            .iter()
            .map(|r| r.to_string())
            .collect::<Vec<String>>()
            .join(",");

        let _ = writeln!(&mut csv, "{header}",);

        if config.range {
            for (i, row) in rows.enumerate() {
                if i > config.end {
                    break;
                }

                let csv_row = row
                    .iter()
                    .map(|r| r.to_string())
                    .collect::<Vec<String>>()
                    .join(",");

                if i >= config.start && i <= config.end {
                    let _ = writeln!(&mut csv, "{csv_row}");
                }
            }
        } else {
            let mut csv_rows = vec![];

            for row in rows.skip(config.start) {
                let csv_row = row
                    .iter()
                    .map(|r| r.to_string())
                    .collect::<Vec<String>>()
                    .join(",");
                csv_rows.push(csv_row);
            }

            csv_rows.truncate(csv_rows.len().saturating_sub(config.end));

            let _ = writeln!(&mut csv, "{}", csv_rows.join("\n"));
        }

        result.push(csv);
    }

    Ok(result.join("\n\n"))
}
