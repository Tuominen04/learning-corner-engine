//! The one typeface a PDF is set in: DejaVu Sans, carried inside the engine.
//!
//! A PDF has to carry its fonts to look the same on every computer, and a font found on this
//! one may be missing on the next. DejaVu Sans is the one bundled because it covers what people
//! write here — Finnish and Swedish, the rest of the Latin alphabets, Greek and Cyrillic — and
//! its licence lets it travel inside a document (`fonts/LICENSE`).
//!
//! Only the glyphs a document uses go into its PDF, so a page of writing carries a few dozen
//! kilobytes of font rather than all 750.
//!
//! There is one face and no bold or italic one. Italics are the upright letters slanted, and
//! bold is the letters drawn with an outline round them — what a word processor does for a
//! font that has no bold or italic of its own.

use std::collections::BTreeMap;
use std::io;

use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Filter, Finish, Name, Pdf, Rect, Ref, Str};
use subsetter::GlyphRemapper;
use ttf_parser::{Face, GlyphId};

const DEJAVU_SANS: &[u8] = include_bytes!("../../../fonts/DejaVuSans.ttf");

/// The glyph a font draws for a character it does not have: an empty box.
const NOTDEF: u16 = 0;

const IDENTITY: SystemInfo = SystemInfo {
    registry: Str(b"Adobe"),
    ordering: Str(b"Identity"),
    supplement: 0,
};

pub(super) struct Font {
    face: Face<'static>,
    /// Font units to the em.
    units: f32,
    /// The glyphs drawn so far, numbered in the order they were first drawn — which is how
    /// they are numbered in the font the PDF carries.
    glyphs: GlyphRemapper,
    /// The character each glyph drawn stands for, so text copied out of the PDF is the text.
    chars: BTreeMap<u16, char>,
}

/// Text ready to draw.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Shown {
    /// Glyphs, two bytes each, numbered as in the font the PDF carries.
    Glyphs(Vec<u8>),
    /// A character the font has no glyph for. It is drawn as the empty box, and carries
    /// itself along so that copying it out of the PDF still gives the character.
    Missing(char),
}

/// Where the PDF's font objects go.
pub(super) struct FontRefs {
    pub font: Ref,
    cid_font: Ref,
    descriptor: Ref,
    to_unicode: Ref,
    file: Ref,
}

impl FontRefs {
    pub fn new(next: &mut Ref) -> Self {
        FontRefs {
            font: next.bump(),
            cid_font: next.bump(),
            descriptor: next.bump(),
            to_unicode: next.bump(),
            file: next.bump(),
        }
    }
}

impl Font {
    pub fn new() -> io::Result<Font> {
        let face = Face::parse(DEJAVU_SANS, 0)
            .map_err(|e| io::Error::other(format!("reading the bundled font: {e}")))?;
        Ok(Font {
            units: f32::from(face.units_per_em()),
            face,
            glyphs: GlyphRemapper::new(),
            chars: BTreeMap::new(),
        })
    }

    fn glyph(&self, c: char) -> u16 {
        self.face.glyph_index(c).map_or(NOTDEF, |g| g.0)
    }

    fn advance_of(&self, glyph: u16) -> f32 {
        f32::from(self.face.glyph_hor_advance(GlyphId(glyph)).unwrap_or(0)) / self.units
    }

    /// How far a character moves the pen, in ems: nothing for one that is never drawn.
    pub fn advance(&self, c: char) -> f32 {
        if invisible(c) {
            0.0
        } else {
            self.advance_of(self.glyph(c))
        }
    }

    /// How wide `text` is, set at `size` points.
    pub fn width(&self, text: &str, size: f32) -> f32 {
        text.chars().map(|c| self.advance(c)).sum::<f32>() * size
    }

    /// From the baseline up to the top of the tallest letters, in ems.
    pub fn ascent(&self) -> f32 {
        f32::from(self.face.ascender()) / self.units
    }

    /// From the baseline down to the bottom of the deepest letters, in ems: below zero.
    pub fn descent(&self) -> f32 {
        f32::from(self.face.descender()) / self.units
    }

    /// Where a line under the text goes, and how thick it is: the font's own, in ems.
    pub fn underline(&self) -> (f32, f32) {
        self.face.underline_metrics().map_or((-0.1, 0.05), |m| {
            (
                f32::from(m.position) / self.units,
                f32::from(m.thickness) / self.units,
            )
        })
    }

    /// Text as the glyphs that draw it, noting each one for the PDF's font.
    pub fn shape(&mut self, text: &str) -> Vec<Shown> {
        let mut shown = Vec::new();
        let mut glyphs = Vec::new();
        for c in text.chars().filter(|&c| !invisible(c)) {
            let glyph = self.glyph(c);
            if glyph == NOTDEF {
                if !glyphs.is_empty() {
                    shown.push(Shown::Glyphs(std::mem::take(&mut glyphs)));
                }
                shown.push(Shown::Missing(c));
                continue;
            }
            let id = self.glyphs.remap(glyph);
            self.chars.entry(id).or_insert(c);
            glyphs.extend(id.to_be_bytes());
        }
        if !glyphs.is_empty() {
            shown.push(Shown::Glyphs(glyphs));
        }
        shown
    }

    /// The font as the PDF carries it: only the glyphs drawn, with their widths and the
    /// characters they stand for. Written last, once every page has been drawn.
    pub fn write(&self, pdf: &mut Pdf, refs: &FontRefs) -> io::Result<()> {
        let subset = subsetter::subset(DEJAVU_SANS, 0, &self.glyphs)
            .map_err(|e| io::Error::other(format!("taking the glyphs from the font: {e}")))?;
        let name = format!("{}+DejaVuSans", self.tag());
        let name = Name(name.as_bytes());
        let em = |units: i16| f32::from(units) / self.units * 1000.0;

        pdf.type0_font(refs.font)
            .base_font(name)
            .encoding_predefined(Name(b"Identity-H"))
            .descendant_font(refs.cid_font)
            .to_unicode(refs.to_unicode);

        let widths: Vec<f32> = self
            .glyphs
            .remapped_gids()
            .map(|glyph| self.advance_of(glyph) * 1000.0)
            .collect();
        let mut cid_font = pdf.cid_font(refs.cid_font);
        cid_font
            .subtype(CidFontType::Type2)
            .base_font(name)
            .system_info(IDENTITY)
            .font_descriptor(refs.descriptor)
            .default_width(0.0)
            .cid_to_gid_map_predefined(Name(b"Identity"));
        cid_font.widths().consecutive(0, widths);
        cid_font.finish();

        let bbox = self.face.global_bounding_box();
        pdf.font_descriptor(refs.descriptor)
            .name(name)
            .flags(FontFlags::SYMBOLIC)
            .bbox(Rect::new(
                em(bbox.x_min),
                em(bbox.y_min),
                em(bbox.x_max),
                em(bbox.y_max),
            ))
            .italic_angle(0.0)
            .ascent(em(self.face.ascender()))
            .descent(em(self.face.descender()))
            .cap_height(em(self
                .face
                .capital_height()
                .unwrap_or(self.face.ascender())))
            .stem_v(80.0)
            .font_file2(refs.file);

        let mut cmap = UnicodeCmap::new(Name(b"Custom"), IDENTITY);
        for (&id, &c) in &self.chars {
            cmap.pair(id, c);
        }
        let cmap = super::deflate(cmap.finish().as_slice())?;
        pdf.cmap(refs.to_unicode, &cmap).filter(Filter::FlateDecode);

        let file = super::deflate(&subset)?;
        pdf.stream(refs.file, &file)
            .filter(Filter::FlateDecode)
            .pair(
                Name(b"Length1"),
                i32::try_from(subset.len()).unwrap_or(i32::MAX),
            );
        Ok(())
    }

    /// The six capital letters a font carried in part is named with. Taken from the glyphs
    /// it holds, so the same document always makes the same name — and so the same bytes.
    fn tag(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        for glyph in self.glyphs.remapped_gids() {
            hasher.update(&glyph.to_be_bytes());
        }
        hasher.finalize().as_bytes()[..6]
            .iter()
            .map(|b| char::from(b'A' + b % 26))
            .collect()
    }
}

/// Characters that take no room and are never drawn: control characters, and the invisible
/// marks that only steer the text around them — a soft hyphen, a zero-width space, a joiner,
/// a direction mark, the selector that asks for an emoji's colour.
pub(super) fn invisible(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{ad}'
                | '\u{34f}'
                | '\u{200b}'..='\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{fe00}'..='\u{fe0f}'
                | '\u{feff}'
                | '\u{e0100}'..='\u{e01ef}'
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_font_has_every_letter_of_the_languages_written_here() {
        let font = Font::new().unwrap();
        let alphabets = [
            "abcdefghijklmnopqrstuvwxyzåäöšž ABCDEFGHIJKLMNOPQRSTUVWXYZÅÄÖŠŽ",
            "абвгдеёжзийклмнопрстуфхцчшщъыьэюя АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯ",
            "αβγδεζηθικλμνξοπρστυφχψω ΑΒΓΔΕΖΗΘΙΚΛΜΝΞΟΠΡΣΤΥΦΧΨΩ",
            "0123456789 .,;:!?()[]&–—‘’“”«»…€§%",
        ];
        for c in alphabets.concat().chars() {
            assert_ne!(font.glyph(c), NOTDEF, "{c:?}");
        }
        assert_eq!(font.glyph('漢'), NOTDEF);
    }

    #[test]
    fn widths_are_the_fonts_own() {
        let font = Font::new().unwrap();
        // DejaVu Sans: a space is 651 units of 2048, an "m" 1995.
        assert!((font.width(" ", 1000.0) - 651.0 / 2048.0 * 1000.0).abs() < 0.01);
        assert!((font.width("mm", 10.0) - 2.0 * 1995.0 / 2048.0 * 10.0).abs() < 0.01);
        assert_eq!(font.width("\u{ad}\u{200b}\t", 12.0), 0.0);
        assert!(font.ascent() > 0.0 && font.descent() < 0.0);
    }

    #[test]
    fn glyphs_are_numbered_in_the_order_they_are_first_drawn() {
        let mut font = Font::new().unwrap();
        // The empty box is always glyph 0, so the first letter drawn is 1.
        assert_eq!(font.shape("aba"), [Shown::Glyphs(vec![0, 1, 0, 2, 0, 1])]);
        assert_eq!(font.chars.get(&1), Some(&'a'));
        assert_eq!(font.chars.get(&2), Some(&'b'));
    }

    #[test]
    fn a_character_the_font_lacks_is_drawn_as_the_box_and_kept_as_itself() {
        let mut font = Font::new().unwrap();
        assert_eq!(
            font.shape("a漢\u{ad}b"),
            [
                Shown::Glyphs(vec![0, 1]),
                Shown::Missing('漢'),
                Shown::Glyphs(vec![0, 2]),
            ]
        );
        // The box stands for many characters, so it stands for none in particular.
        assert_eq!(font.chars.get(&0), None);
    }

    #[test]
    fn the_font_a_pdf_carries_keeps_its_copyright_and_licence() {
        // The licence asks that its notices go with every copy of the font, and the part of it
        // inside a PDF is a copy.
        let mut font = Font::new().unwrap();
        font.shape("Protein");
        let subset = subsetter::subset(DEJAVU_SANS, 0, &font.glyphs).unwrap();
        let face = Face::parse(&subset, 0).unwrap();
        let name = |id: u16| {
            face.names()
                .into_iter()
                .find(|name| name.name_id == id && name.is_unicode())
                .and_then(|name| name.to_string())
                .unwrap_or_default()
        };
        assert!(name(ttf_parser::name_id::COPYRIGHT_NOTICE).contains("Bitstream"));
        assert!(name(ttf_parser::name_id::LICENSE).contains("public domain"));
        assert!(
            subset.len() < DEJAVU_SANS.len() / 10,
            "{} bytes",
            subset.len()
        );
    }

    #[test]
    fn the_same_glyphs_make_the_same_name() {
        let mut one = Font::new().unwrap();
        let mut two = Font::new().unwrap();
        one.shape("Protein");
        two.shape("Protein");
        assert_eq!(one.tag(), two.tag());
        assert!(one.tag().chars().all(|c| c.is_ascii_uppercase()));
        assert_eq!(one.tag().len(), 6);
        two.shape("Белок");
        assert_ne!(one.tag(), two.tag());
    }
}
