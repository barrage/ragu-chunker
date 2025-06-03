use super::{DocumentPage, DocumentSection, SectionParseConfig, StringParseConfig};
use crate::{
    core::{
        document::{parser::PageRange, sha256},
        model::image::Image,
    },
    error::ChonkitError,
    map_err,
};
use pdfium_render::prelude::{PdfPage, PdfPageObject, PdfPageObjectsCommon, Pdfium};
use regex::Regex;
use std::{fmt::Write, time::Instant};
use tracing::debug;

/// Parser implementation that reads the _whole_ PDF document and extracts its text to a single string.
///
/// Configuration:
///
/// * `start`: The amount of PDF pages to skip from the start of the document.
/// * `end`: The amount of pages to omit from the back of the document.
/// * `range`: If `true`, `skip_start` and `skip_end` are treated as a range.
/// * `filters`: Line based, i.e. lines matching a filter will be skipped.
pub async fn parse_to_string(
    config: &StringParseConfig,
    input: &[u8],
    include_images: bool,
) -> Result<(String, Vec<Image>), ChonkitError> {
    // For debugging
    let mut _page_count = 0;
    let _start = Instant::now();

    let hash = sha256(input);
    let pdfium = Pdfium::default();
    let input = map_err!(pdfium.load_pdf_from_byte_slice(input, None));

    let filters: Vec<Regex> = config
        .filters
        .iter()
        .filter_map(|re| Regex::new(re).ok())
        .collect();

    let mut out = String::new();
    let mut images = vec![];

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

        if !include_images {
            continue;
        };

        process_page_images(&page, &hash, page_num, &mut images);
    }

    debug!(
        "Finished processing PDF, {_page_count}/{total_pages} pages took {}ms",
        _start.elapsed().as_millis()
    );

    Ok((out, images))
}

/// Parser implementation that reads PDF sections (pages) and outputs their text content in
/// a paginated format, i.e. [DocumentSection].
pub async fn parse_to_sections(
    config: &SectionParseConfig,
    input: &[u8],
    include_images: bool,
) -> Result<Vec<DocumentSection>, ChonkitError> {
    let _start = Instant::now();

    let filters: Vec<Regex> = config
        .filters
        .iter()
        .filter_map(|re| Regex::new(re).ok())
        .collect();

    let pdfium = Pdfium::default();
    let hash = sha256(input);
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

            let mut page = DocumentPage {
                content: out,
                number: i + 1,
                images: vec![],
            };

            if !include_images {
                section.pages.push(page);
                continue;
            };

            process_page_images(&pdf_page, &hash, i + 1, &mut page.images);
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

fn process_page_images(page: &PdfPage<'_>, hash: &str, page_num: usize, images: &mut Vec<Image>) {
    // Process images
    for (i, object) in page.objects().iter().enumerate() {
        let PdfPageObject::Image(ref pdf_page_image_object) = object else {
            continue;
        };

        let img_path = format!("{}_{}_{}.webp", hash, page_num, i);

        match pdf_page_image_object.get_raw_bitmap() {
            Ok(bitmap) => {
                let mut bytes = vec![];
                let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut bytes);
                encoder
                    .encode(
                        &bitmap.as_rgba_bytes(),
                        bitmap.width() as u32,
                        bitmap.height() as u32,
                        image::ExtendedColorType::Rgba8,
                    )
                    .unwrap();

                let image = Image::new(img_path, bytes, image::ImageFormat::WebP);

                if image.size_in_mb() > 20 {
                    tracing::warn!("Image too large: {image} bytes (page: {})", page_num + 1);
                    continue;
                }

                images.push(image);
            }
            Err(err) => {
                debug!("Error getting image: {err}");
            }
        }
    }
}
