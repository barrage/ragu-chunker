use super::{DocumentPage, DocumentSection, GenericParseConfig, SectionParseConfig};
use crate::{core::document::parser::PageRange, error::ChonkitError, map_err};
use pdfium_render::prelude::Pdfium;
use regex::Regex;
use std::{fmt::Write, time::Instant};
use tracing::debug;

pub fn parse_paginated(
    input: &[u8],
    config: &SectionParseConfig,
) -> Result<Vec<DocumentSection>, ChonkitError> {
    let _start = Instant::now();
    let filters: Vec<Regex> = config
        .filters
        .iter()
        .filter_map(|re| Regex::new(re).ok())
        .collect();

    let pdfium = Pdfium::default();
    let input = map_err!(pdfium.load_pdf_from_byte_slice(input, None));

    let pages = input.pages();

    let mut sections = vec![];
    let mut section = DocumentSection::default();

    for range in config.sections.iter() {
        let PageRange { start, end } = *range;

        debug_assert!(start > 0);
        debug_assert!(end >= start);

        for i in start - 1..end {
            if i >= pages.len() as usize {
                break;
            }

            let page = map_err!(pages.get(i as u16));
            let text = map_err!(page.text());
            let mut out = String::new();

            'lines: for line in text.all().lines() {
                let line = line.trim();

                for filter in filters.iter() {
                    if filter.is_match(line) {
                        continue 'lines;
                    }
                }

                let _ = writeln!(out, "{line}");
            }

            section.pages.push(DocumentPage {
                content: out,
                number: i + 1,
            });
        }

        sections.push(section);
        section = DocumentSection::default();
    }

    debug!(
        "Finished processing PDF, took {}ms",
        _start.elapsed().as_millis()
    );

    Ok(sections)
}

/// Generic PDF parser.
///
/// Configuration:
///
/// * `start`: The amount of PDF pages to skip from the start of the document.
/// * `end`: The amount of pages to omit from the back of the document.
/// * `range`: If `true`, `skip_start` and `skip_end` are treated as a range.
/// * `filters`: Line based, i.e. lines matching a filter will be skipped.
pub fn parse(input: &[u8], config: &GenericParseConfig) -> Result<String, ChonkitError> {
    let _start = Instant::now();

    let start = config.start;
    let end = config.end;
    let filters: Vec<Regex> = config
        .filters
        .iter()
        .filter_map(|re| Regex::new(re).ok())
        .collect();

    let range = config.range;

    let pdfium = Pdfium::default();
    let input = map_err!(pdfium.load_pdf_from_byte_slice(input, None));

    let mut out = String::new();

    let pages = input.pages();

    let total_pages = pages.len();

    let start = if range { start - 1 } else { start };
    let end_condition: Box<dyn Fn(usize) -> bool> = if range {
        Box::new(|page_num| page_num == end.saturating_sub(1))
    } else {
        Box::new(|page_num| {
            total_pages
                .saturating_sub(page_num as u16)
                .saturating_sub(end as u16)
                == 0
        })
    };

    // For debugging
    let mut page_count = 0;

    for (page_num, page) in pages.iter().enumerate().skip(start) {
        if end_condition(page_num) {
            break;
        }

        // page_num is 0 based
        let text = map_err!(page.text());

        'lines: for line in text.all().lines() {
            let line = line.trim();

            for filter in filters.iter() {
                if filter.is_match(line) {
                    continue 'lines;
                }
            }

            let _ = writeln!(out, "{line}");
        }

        page_count += 1;
    }

    debug!(
        "Finished processing PDF, {page_count}/{total_pages} pages took {}ms",
        _start.elapsed().as_millis()
    );

    Ok(out)
}
