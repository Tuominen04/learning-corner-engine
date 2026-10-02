//! A PDF: the document exactly as it prints, the same on every computer, for whoever only needs
//! to read it.
//!
//! Everything is laid out here rather than left to a reader: the lines are broken, the pages
//! are broken, and the one font is carried inside the file (`font.rs`). Pages are A4 with inch
//! margins and numbered at the top right, as APA puts the number. The headings are the PDF's
//! outline, so a reader's sidebar lists them, and the reference list's web addresses are links.
//!
//! The same document always makes the same bytes: there is no date in the file, and nothing in
//! it depends on the order a hash map happens to keep.

mod font;
mod layout;
mod picture;

use std::io::{self, Write};

use flate2::write::ZlibEncoder;
use flate2::Compression;
use pdf_writer::types::{ActionType, AnnotationType, Predictor, TextRenderingMode};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str, TextStr};

use super::Export;
use font::{Font, FontRefs, Shown};
use layout::{Item, Layout, Look, MARGIN, PAGE_HEIGHT, PAGE_WIDTH};
use picture::{Colour, Data};

/// The font's name in each page's resources.
const FONT: Name = Name(b"F");

/// How far italics lean: the horizontal shift per unit of height, about eleven degrees — the
/// slant of DejaVu Sans's own oblique.
const SLANT: f32 = 0.2;

/// How thick the outline drawn round a bold letter is, as a fraction of the text's size.
const EMBOLDEN: f32 = 0.05;

/// The blue of a link: Word's own.
const LINK: (f32, f32, f32) = (0.02, 0.388, 0.757);

/// The grey of what stands in for a missing picture.
const FAINT: f32 = 0.4;

pub fn write(export: &Export) -> io::Result<Vec<u8>> {
    let mut font = Font::new()?;
    let layout = layout::lay_out(export, &font);

    let mut next = Ref::new(1);
    let catalog = next.bump();
    let tree = next.bump();
    let info = next.bump();
    let font_refs = FontRefs::new(&mut next);
    let pages: Vec<(Ref, Ref)> = layout
        .pages
        .iter()
        .map(|_| (next.bump(), next.bump()))
        .collect();
    let pictures: Vec<(Ref, Option<Ref>)> = layout
        .pictures
        .iter()
        .map(|p| (next.bump(), p.alpha.as_ref().map(|_| next.bump())))
        .collect();

    let mut pdf = Pdf::new();
    pdf.pages(tree)
        .kids(pages.iter().map(|&(page, _)| page))
        .count(i32::try_from(pages.len()).unwrap_or(i32::MAX));

    for (number, (items, &(page_ref, content_ref))) in layout.pages.iter().zip(&pages).enumerate() {
        let content = deflate(&draw(items, number + 1, &mut font).finish())?;
        pdf.stream(content_ref, &content)
            .filter(Filter::FlateDecode);

        let links: Vec<Ref> = layout
            .links
            .iter()
            .filter(|link| link.page == number)
            .map(|link| {
                let id = next.bump();
                let mut annotation = pdf.annotation(id);
                annotation
                    .subtype(AnnotationType::Link)
                    .rect(Rect::new(
                        link.left,
                        PAGE_HEIGHT - link.bottom,
                        link.right,
                        PAGE_HEIGHT - link.top,
                    ))
                    .border(0.0, 0.0, 0.0, None);
                annotation
                    .action()
                    .action_type(ActionType::Uri)
                    .uri(Str(link.url.as_bytes()));
                annotation.finish();
                id
            })
            .collect();

        let mut page = pdf.page(page_ref);
        page.parent(tree)
            .media_box(Rect::new(0.0, 0.0, PAGE_WIDTH, PAGE_HEIGHT))
            .contents(content_ref);
        let mut resources = page.resources();
        resources.fonts().pair(FONT, font_refs.font);
        let shown: Vec<usize> = items
            .iter()
            .filter_map(|item| match item {
                Item::Picture { index, .. } => Some(*index),
                Item::Text { .. } => None,
            })
            .collect();
        if !shown.is_empty() {
            let mut objects = resources.x_objects();
            for index in shown {
                objects.pair(Name(picture_name(index).as_bytes()), pictures[index].0);
            }
        }
        resources.finish();
        if !links.is_empty() {
            page.annotations(links);
        }
    }

    for (prepared, &(id, mask)) in layout.pictures.iter().zip(&pictures) {
        let (width, height) = prepared.pixels;
        let (width, height) = (
            i32::try_from(width).unwrap_or(i32::MAX),
            i32::try_from(height).unwrap_or(i32::MAX),
        );
        let channels = i32::try_from(prepared.colour.channels()).unwrap_or(1);
        let mut image = match &prepared.data {
            Data::Jpeg(bytes) => {
                let mut image = pdf.image_xobject(id, bytes);
                image.filter(Filter::DctDecode);
                image
            }
            Data::Predicted(bytes) => {
                let mut image = pdf.image_xobject(id, bytes);
                image.filter(Filter::FlateDecode);
                predicted(image.decode_parms(), channels, width);
                image
            }
        };
        image.width(width).height(height).bits_per_component(8);
        match prepared.colour {
            Colour::Grey => image.color_space().device_gray(),
            Colour::Rgb => image.color_space().device_rgb(),
            Colour::Cmyk { inverted } => {
                image.color_space().device_cmyk();
                if inverted {
                    image.decode([1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0]);
                }
            }
        }
        if let Some(mask) = mask {
            image.s_mask(mask);
        }
        image.finish();
        if let (Some(mask), Some(alpha)) = (mask, &prepared.alpha) {
            let mut opacity = pdf.image_xobject(mask, alpha);
            opacity.width(width).height(height).bits_per_component(8);
            opacity.filter(Filter::FlateDecode);
            predicted(opacity.decode_parms(), 1, width);
            opacity.color_space().device_gray();
            opacity.finish();
        }
    }

    font.write(&mut pdf, &font_refs)?;

    let outline = outline(&mut pdf, &mut next, &layout, &pages);
    let mut root = pdf.catalog(catalog);
    root.pages(tree);
    if let Some(outline) = outline {
        root.outlines(outline);
    }
    // A reader's title bar shows the document's title rather than the file's name.
    root.viewer_preferences().display_doc_title(true);
    root.finish();
    pdf.document_info(info).title(TextStr(&export.title));
    Ok(pdf.finish())
}

/// How to undo PNG's filtering, row by row, after inflating a picture's pixels.
fn predicted(mut parameters: pdf_writer::writers::DecodeParms, channels: i32, width: i32) {
    parameters
        .predictor(Predictor::PngOptimum)
        .colors(channels)
        .bits_per_component(8)
        .columns(width);
}

fn picture_name(index: usize) -> String {
    format!("P{}", index + 1)
}

/// A page's content: its page number, then everything on it, in reading order.
fn draw(items: &[Item], number: usize, font: &mut Font) -> Content {
    let mut content = Content::new();
    let mut pen = Pen::default();

    let size = 11.0;
    let number = number.to_string();
    let width = font.width(&number, size);
    let baseline = MARGIN / 2.0 + font.ascent() * size;
    pen.text(
        &mut content,
        font,
        PAGE_WIDTH - MARGIN - width,
        baseline,
        size,
        Look::default(),
        &number,
    );

    for item in items {
        match item {
            Item::Text {
                x,
                baseline,
                width,
                size,
                look,
                text,
            } => {
                pen.text(&mut content, font, *x, *baseline, *size, *look, text);
                if look.link {
                    let (position, thickness) = font.underline();
                    content
                        .rect(
                            *x,
                            PAGE_HEIGHT - baseline + (position - thickness / 2.0) * size,
                            *width,
                            thickness * size,
                        )
                        .fill_nonzero();
                }
            }
            Item::Picture {
                x,
                top,
                width,
                height,
                index,
            } => {
                content
                    .save_state()
                    .transform([*width, 0.0, 0.0, *height, *x, PAGE_HEIGHT - top - height])
                    .x_object(Name(picture_name(*index).as_bytes()))
                    .restore_state();
            }
        }
    }
    content
}

/// What the content stream has set so far, so that each change is written once.
#[derive(Default)]
struct Pen {
    colour: Option<(f32, f32, f32)>,
    bold: Option<bool>,
}

impl Pen {
    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        content: &mut Content,
        font: &mut Font,
        x: f32,
        baseline: f32,
        size: f32,
        look: Look,
        text: &str,
    ) {
        let colour = if look.link {
            LINK
        } else if look.faint {
            (FAINT, FAINT, FAINT)
        } else {
            (0.0, 0.0, 0.0)
        };
        if self.colour != Some(colour) {
            let (r, g, b) = colour;
            content.set_fill_rgb(r, g, b).set_stroke_rgb(r, g, b);
            self.colour = Some(colour);
        }
        if self.bold != Some(look.bold) {
            if look.bold {
                content.set_text_rendering_mode(TextRenderingMode::FillStroke);
            } else {
                content.set_text_rendering_mode(TextRenderingMode::Fill);
            }
            self.bold = Some(look.bold);
        }
        if look.bold {
            content.set_line_width(EMBOLDEN * size);
        }

        content.begin_text();
        content.set_font(FONT, size);
        let slant = if look.italic { SLANT } else { 0.0 };
        content.set_text_matrix([1.0, 0.0, slant, 1.0, x, PAGE_HEIGHT - baseline]);
        for shown in font.shape(text) {
            match shown {
                Shown::Glyphs(glyphs) => {
                    content.show(Str(&glyphs));
                }
                Shown::Missing(c) => {
                    content
                        .begin_marked_content_with_properties(Name(b"Span"))
                        .properties()
                        .actual_text(TextStr(c.encode_utf8(&mut [0; 4])));
                    content.show(Str(&[0, 0]));
                    content.end_marked_content();
                }
            }
        }
        content.end_text();
    }
}

/// The PDF's outline: every heading, nested as the headings are, each leading to its page.
fn outline(pdf: &mut Pdf, next: &mut Ref, layout: &Layout, pages: &[(Ref, Ref)]) -> Option<Ref> {
    if layout.headings.is_empty() {
        return None;
    }
    let root = next.bump();
    let ids: Vec<Ref> = layout.headings.iter().map(|_| next.bump()).collect();

    // Each heading's parent: the nearest heading above it of a higher level.
    let mut parents: Vec<Option<usize>> = Vec::new();
    let mut open: Vec<usize> = Vec::new();
    for (i, heading) in layout.headings.iter().enumerate() {
        while open
            .last()
            .is_some_and(|&j| layout.headings[j].level >= heading.level)
        {
            open.pop();
        }
        parents.push(open.last().copied());
        open.push(i);
    }
    let children = |parent: Option<usize>| -> Vec<usize> {
        (0..ids.len()).filter(|&i| parents[i] == parent).collect()
    };
    // How many headings are below one, however deep: all of them show, the outline open.
    let descendants = |i: usize| -> usize {
        (i + 1..ids.len())
            .filter(|&later| {
                let mut up = parents[later];
                while let Some(p) = up {
                    if p == i {
                        return true;
                    }
                    up = parents[p];
                }
                false
            })
            .count()
    };

    let top = children(None);
    let (Some(&first), Some(&last)) = (top.first(), top.last()) else {
        return None;
    };
    pdf.outline(root)
        .first(ids[first])
        .last(ids[last])
        .count(i32::try_from(ids.len()).unwrap_or(i32::MAX));

    for (i, heading) in layout.headings.iter().enumerate() {
        let siblings = children(parents[i]);
        let place = siblings.iter().position(|&s| s == i).unwrap_or(0);
        let mut item = pdf.outline_item(ids[i]);
        item.title(TextStr(&heading.title))
            .parent(parents[i].map_or(root, |p| ids[p]));
        if place > 0 {
            item.prev(ids[siblings[place - 1]]);
        }
        if let Some(&after) = siblings.get(place + 1) {
            item.next(ids[after]);
        }
        let below = children(Some(i));
        if let (Some(&first), Some(&last)) = (below.first(), below.last()) {
            item.first(ids[first]).last(ids[last]);
            item.count(i32::try_from(descendants(i)).unwrap_or(i32::MAX));
        }
        item.dest()
            .page(pages[heading.page].0)
            .xyz(0.0, PAGE_HEIGHT - heading.top + 6.0, None);
    }
    Some(root)
}

/// Compressed, as nearly every stream in the file is.
fn deflate(data: &[u8]) -> io::Result<Vec<u8>> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data)?;
    encoder.finish()
}
