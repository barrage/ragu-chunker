use super::{DocumentPage, DocumentSection, SectionParseConfig, StringParseConfig};
use crate::{
    core::{document::parser::PageRange, model::image::Image},
    error::ChonkitError,
    map_err,
};
use pdfium_render::prelude::{PdfPageObject, PdfPageObjectsCommon, Pdfium};
use regex::Regex;
use std::{collections::HashSet, fmt::Write, time::Instant};
use tracing::debug;

/// Parser implementation that reads the _whole_ PDF document and extracts its text to a single string.
///
/// Configuration:
///
/// * `start`: The amount of PDF pages to skip from the start of the document.
/// * `end`: The amount of pages to omit from the back of the document.
/// * `range`: If `true`, `skip_start` and `skip_end` are treated as a range.
/// * `filters`: Line based, i.e. lines matching a filter will be skipped.
pub(super) fn parse_to_string(
    config: &StringParseConfig,
    input: &[u8],
) -> Result<String, ChonkitError> {
    let _start = Instant::now();
    let mut _page_count = 0;

    let pdfium = Pdfium::default();
    let input = map_err!(pdfium.load_pdf_from_byte_slice(input, None));

    let filters: Vec<Regex> = config
        .filters
        .iter()
        .filter_map(|re| Regex::new(re).ok())
        .collect();

    let mut out = String::new();

    let pages = input.pages();

    let total_pages = pages.len();

    let start = if config.range {
        config.start - 1
    } else {
        config.start
    };

    let end_condition: &dyn Fn(usize) -> bool = if config.range {
        &|page_num| page_num == config.end.saturating_sub(1)
    } else {
        &|page_num| {
            total_pages
                .saturating_sub(page_num as u16)
                .saturating_sub(config.end as u16)
                == 0
        }
    };

    // page_num is 0 indexed
    for (page_num, page) in pages.iter().enumerate().skip(start) {
        if end_condition(page_num) {
            break;
        }

        _page_count += 1;

        // Process text line by line and apply filters
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
    }

    debug!(
        "Finished processing PDF, {_page_count}/{total_pages} pages took {}ms",
        _start.elapsed().as_millis()
    );

    Ok(out)
}

/// Parser implementation that reads PDF sections (pages) and outputs their text content in
/// a paginated format, i.e. [DocumentSection].
pub(super) fn parse_to_sections(
    config: &SectionParseConfig,
    input: &[u8],
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

            let pdf_page = map_err!(pages.get(i as u16));
            let text = map_err!(pdf_page.text());
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

/// Implementation that goes through the whole document to extract images.
///
/// The `skip` set contains images already parsed and is usually obtained from the database.
/// The set should be empty during initial parsing. The set should consist of the page number
/// and image number combination for a specific document. This metadata is stored with every
/// image obtained from the document in upstream layers.
pub(super) fn parse_images(
    input: &[u8],
    skip: &HashSet<(usize, usize)>,
) -> Result<Vec<Image>, ChonkitError> {
    let pdfium = Pdfium::default();
    let input = map_err!(pdfium.load_pdf_from_byte_slice(input, None));

    let pages = input.pages();
    let total_pages = pages.len();

    let mut images = vec![];

    for (page_num, page) in pages.iter().enumerate() {
        let len_pre = images.len();

        let mut image_num = 0;
        for object in page.objects().iter() {
            let PdfPageObject::Image(ref pdf_page_image_object) = object else {
                continue;
            };

            if skip.contains(&(page_num, image_num)) {
                image_num += 1;
                continue;
            }

            match pdf_page_image_object.get_raw_bitmap() {
                Ok(bitmap) => {
                    let mut bytes = vec![];
                    let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut bytes);
                    let width = bitmap.width() as u32;
                    let height = bitmap.height() as u32;

                    encoder
                        .encode(
                            &bitmap.as_rgba_bytes(),
                            width,
                            height,
                            image::ExtendedColorType::Rgba8,
                        )
                        .unwrap();

                    images.push(Image::new(
                        Some(page_num),
                        Some(image_num),
                        bytes,
                        image::ImageFormat::WebP,
                        width,
                        height,
                    ));

                    image_num += 1;
                }
                Err(err) => {
                    debug!("Error getting image: {err}");
                }
            }
        }

        if images.len() > len_pre {
            tracing::debug!(
                "Page {page_num}/{total_pages} - parsed {} images",
                images.len() - len_pre
            );
        }
    }

    Ok(images)
}
