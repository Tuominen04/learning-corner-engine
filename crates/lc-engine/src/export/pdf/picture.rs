//! Pictures as a PDF carries them.
//!
//! A JPEG goes in as it is, because PDF reads JPEG itself. A PNG or a GIF is read into its
//! pixels and compressed again, with any transparency beside the colours as a mask of its own,
//! because that is how PDF keeps them. A picture that cannot be carried — a WebP, a rare kind
//! of JPEG that PDF readers cannot be trusted with, a damaged file — is written as its
//! description instead, as in the Word and OpenDocument files.

use std::io::Cursor;

use crate::export::{jpeg_frame, ImageKind, Picture};

/// The most pixels a picture is unpacked to: 64 million, a quarter of a gigabyte in memory.
/// Far more than any screenshot or drawing; a photograph is a JPEG, which is never unpacked.
const MOST_PIXELS: u64 = 64_000_000;

pub(super) struct Prepared<'a> {
    /// Width and height on the page, in points.
    pub size: (f32, f32),
    /// Width and height in pixels.
    pub pixels: (u32, u32),
    pub colour: Colour,
    pub data: Data<'a>,
    /// How opaque each pixel is, compressed — when any of them is not entirely.
    pub alpha: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Colour {
    Grey,
    Rgb,
    /// Four inks. `inverted` when Adobe's software wrote it, which stores CMYK upside down.
    Cmyk {
        inverted: bool,
    },
}

impl Colour {
    /// Samples to a pixel.
    pub fn channels(self) -> usize {
        match self {
            Colour::Grey => 1,
            Colour::Rgb => 3,
            Colour::Cmyk { .. } => 4,
        }
    }
}

pub(super) enum Data<'a> {
    /// The JPEG file, exactly as it is.
    Jpeg(&'a [u8]),
    /// The pixels, each row filtered as PNG filters it and then compressed — a good deal
    /// smaller than the pixels compressed as they are. The image's `/DecodeParms` tell a
    /// reader how to undo it.
    Predicted(Vec<u8>),
}

/// The picture ready for the PDF, at its size on the page — or `None` when it can only be
/// described.
pub(super) fn prepare(picture: &Picture) -> Option<Prepared<'_>> {
    let placed = picture.placed()?;
    let bytes = placed.image.bytes.as_slice();
    let (pixels, colour, data, alpha) = match placed.image.kind {
        ImageKind::Jpeg => jpeg(bytes)?,
        ImageKind::Png => png(bytes)?,
        ImageKind::Gif => gif(bytes)?,
        ImageKind::Webp => return None,
    };
    let (width, height) = placed.size;
    Some(Prepared {
        size: ((width * 72.0) as f32, (height * 72.0) as f32),
        pixels,
        colour,
        data,
        alpha,
    })
}

type Read<'a> = ((u32, u32), Colour, Data<'a>, Option<Vec<u8>>);

fn too_big(width: u32, height: u32) -> bool {
    u64::from(width) * u64::from(height) > MOST_PIXELS
}

fn jpeg(bytes: &[u8]) -> Option<Read<'_>> {
    let frame = jpeg_frame(bytes)?;
    // Baseline, extended and progressive, eight bits to a sample: the JPEG every PDF reader
    // reads. The lossless and arithmetic-coded kinds are rare, and many readers cannot.
    if !matches!(frame.marker, 0xc0..=0xc2) || frame.precision != 8 {
        return None;
    }
    let colour = match frame.components {
        Some(1) => Colour::Grey,
        Some(3) => Colour::Rgb,
        Some(4) => Colour::Cmyk {
            inverted: frame.adobe,
        },
        _ => return None,
    };
    Some(((frame.width, frame.height), colour, Data::Jpeg(bytes), None))
}

fn png(bytes: &[u8]) -> Option<Read<'static>> {
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: usize::try_from(MOST_PIXELS * 4).ok()?,
        },
    );
    // Eight bits to a sample, palettes looked up and transparency made an alpha channel:
    // grey or colour, with or without alpha, whatever the file stored.
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let (width, height) = (reader.info().width, reader.info().height);
    if too_big(width, height) {
        return None;
    }
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    buffer.truncate(info.buffer_size());
    let (colour, alpha) = match info.color_type {
        png::ColorType::Grayscale => (Colour::Grey, false),
        png::ColorType::GrayscaleAlpha => (Colour::Grey, true),
        png::ColorType::Rgb => (Colour::Rgb, false),
        png::ColorType::Rgba => (Colour::Rgb, true),
        png::ColorType::Indexed => return None,
    };
    let (data, alpha) = pixels(&buffer, info.width, colour, alpha)?;
    Some(((info.width, info.height), colour, data, alpha))
}

fn gif(bytes: &[u8]) -> Option<Read<'static>> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(bytes).ok()?;
    let (width, height) = (usize::from(decoder.width()), usize::from(decoder.height()));
    if too_big(u32::from(decoder.width()), u32::from(decoder.height())) {
        return None;
    }
    // An animation is shown as its first frame, put where it sits on the picture.
    let frame = decoder.read_next_frame().ok()??;
    let (left, top) = (usize::from(frame.left), usize::from(frame.top));
    let frame_width = usize::from(frame.width);
    let mut canvas = vec![0; width * height * 4];
    if left < width {
        let span = frame_width.min(width - left) * 4;
        for (row, line) in frame.buffer.chunks_exact(frame_width * 4).enumerate() {
            let y = top + row;
            if y >= height {
                break;
            }
            let at = (y * width + left) * 4;
            canvas[at..at + span].copy_from_slice(&line[..span]);
        }
    }
    let (data, alpha) = pixels(&canvas, u32::from(decoder.width()), Colour::Rgb, true)?;
    Some((
        (u32::from(decoder.width()), u32::from(decoder.height())),
        Colour::Rgb,
        data,
        alpha,
    ))
}

/// Pixels as PDF keeps them: the colours, and apart from them the alpha — or no alpha when
/// every pixel is opaque. `width` is in pixels; each sample is a byte.
fn pixels(
    samples: &[u8],
    width: u32,
    colour: Colour,
    alpha: bool,
) -> Option<(Data<'static>, Option<Vec<u8>>)> {
    let width = usize::try_from(width).ok()?;
    let channels = colour.channels();
    let packed = |samples: &[u8], channels: usize| {
        super::deflate(&predicted(samples, width * channels, channels)).ok()
    };
    if !alpha {
        return Some((Data::Predicted(packed(samples, channels)?), None));
    }
    let mut colours = Vec::with_capacity(samples.len() / (channels + 1) * channels);
    let mut opacity = Vec::with_capacity(samples.len() / (channels + 1));
    for pixel in samples.chunks_exact(channels + 1) {
        colours.extend_from_slice(&pixel[..channels]);
        opacity.push(pixel[channels]);
    }
    let mask = if opacity.iter().all(|&a| a == u8::MAX) {
        None
    } else {
        Some(packed(&opacity, 1)?)
    };
    Some((Data::Predicted(packed(&colours, channels)?), mask))
}

/// Rows of `row` bytes filtered the way PNG filters them: each row by whichever of PNG's five
/// filters leaves it the smallest numbers, written after a byte saying which. Neighbouring
/// pixels are alike, so what is left compresses far better than the pixels themselves.
fn predicted(samples: &[u8], row: usize, pixel: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() + samples.len() / row.max(1));
    let mut above = vec![0; row];
    let mut trial = vec![0; row];
    let mut best = vec![0; row];
    for line in samples.chunks_exact(row) {
        let mut best_cost = u64::MAX;
        let mut best_filter = 0;
        for filter in 0..5u8 {
            for i in 0..row {
                let left = if i >= pixel { line[i - pixel] } else { 0 };
                let up = above[i];
                let corner = if i >= pixel { above[i - pixel] } else { 0 };
                let guess = match filter {
                    0 => 0,
                    1 => left,
                    2 => up,
                    3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                    _ => paeth(left, up, corner),
                };
                trial[i] = line[i].wrapping_sub(guess);
            }
            // The usual measure: the residues as signed bytes, the smaller the better.
            let cost = trial
                .iter()
                .map(|&r| u64::from((r as i8).unsigned_abs()))
                .sum();
            if cost < best_cost {
                best_cost = cost;
                best_filter = filter;
                std::mem::swap(&mut best, &mut trial);
            }
        }
        out.push(best_filter);
        out.extend_from_slice(&best);
        above.copy_from_slice(line);
    }
    out
}

/// PNG's Paeth predictor: whichever of left, above and above-left is nearest to their sum
/// less the corner.
fn paeth(left: u8, up: u8, corner: u8) -> u8 {
    let estimate = i16::from(left) + i16::from(up) - i16::from(corner);
    let to = |v: u8| (estimate - i16::from(v)).abs();
    if to(left) <= to(up) && to(left) <= to(corner) {
        left
    } else if to(up) <= to(corner) {
        up
    } else {
        corner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::Image;
    use std::io::Read as _;

    fn inflate(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(data)
            .read_to_end(&mut out)
            .unwrap();
        out
    }

    /// The pixels back from PNG-filtered rows, as a PDF reader gets them: the filter byte
    /// starting each row says how.
    fn unpredict(data: &[u8], row: usize, pixel: usize) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        for (n, line) in data.chunks_exact(row + 1).enumerate() {
            let start = out.len();
            for i in 0..row {
                let left = if i >= pixel {
                    out[start + i - pixel]
                } else {
                    0
                };
                let up = if n > 0 { out[start + i - row] } else { 0 };
                let corner = if n > 0 && i >= pixel {
                    out[start + i - row - pixel]
                } else {
                    0
                };
                let guess = match line[0] {
                    0 => 0,
                    1 => left,
                    2 => up,
                    3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                    4 => paeth_as_specified(left, up, corner),
                    other => panic!("no PNG filter {other}"),
                };
                out.push(line[1 + i].wrapping_add(guess));
            }
        }
        out
    }

    /// The Paeth predictor written out as the PNG specification gives it, apart from the one
    /// the writer uses — so a mistake in that one cannot cancel itself out here.
    fn paeth_as_specified(a: u8, b: u8, c: u8) -> u8 {
        let p = i32::from(a) + i32::from(b) - i32::from(c);
        let pa = (p - i32::from(a)).abs();
        let pb = (p - i32::from(b)).abs();
        let pc = (p - i32::from(c)).abs();
        if pa <= pb && pa <= pc {
            a
        } else if pb <= pc {
            b
        } else {
            c
        }
    }

    #[test]
    fn the_paeth_predictor_is_the_one_png_specifies() {
        // Ties included — 3, 25 and 14 is one — where the order of the comparisons decides.
        for left in (0..=255u8).step_by(3) {
            for up in (0..=255u8).step_by(5) {
                for corner in (0..=255u8).step_by(7) {
                    assert_eq!(
                        paeth(left, up, corner),
                        paeth_as_specified(left, up, corner),
                        "{left} {up} {corner}"
                    );
                }
            }
        }
    }

    #[test]
    fn filtered_rows_come_back_as_the_pixels() {
        // A gradient, some noise and a flat run: each filter wins somewhere.
        let (width, height) = (37, 11);
        let samples: Vec<u8> = (0..width * height * 3)
            .map(|i| match (i / (width * 3)) % 3 {
                0 => (i % 251) as u8,
                1 => ((i * 7919) % 256) as u8,
                _ => 200,
            })
            .collect();
        let rows = predicted(&samples, width * 3, 3);
        assert_eq!(rows.len(), samples.len() + height);
        assert_eq!(unpredict(&rows, width * 3, 3), samples);
        let filters: std::collections::BTreeSet<u8> =
            rows.chunks_exact(width * 3 + 1).map(|r| r[0]).collect();
        assert!(filters.len() > 1, "{filters:?}");
    }

    /// What `prepare` makes of a file, unpacked again to compare.
    #[derive(Debug)]
    struct Seen {
        pixels: (u32, u32),
        size: (f32, f32),
        colour: Colour,
        /// The file itself, when it went in as it is.
        as_it_is: Option<Vec<u8>>,
        colours: Option<Vec<u8>>,
        alpha: Option<Vec<u8>>,
    }

    fn seen(kind: ImageKind, bytes: Vec<u8>) -> Option<Seen> {
        let picture = Picture {
            alt: "A picture".into(),
            name: "picture".into(),
            image: Some(Image { kind, bytes }),
        };
        let prepared = prepare(&picture)?;
        let row = usize::try_from(prepared.pixels.0).unwrap();
        let channels = prepared.colour.channels();
        let (as_it_is, colours) = match &prepared.data {
            Data::Jpeg(bytes) => (Some(bytes.to_vec()), None),
            Data::Predicted(bytes) => (
                None,
                Some(unpredict(&inflate(bytes), row * channels, channels)),
            ),
        };
        Some(Seen {
            pixels: prepared.pixels,
            size: prepared.size,
            colour: prepared.colour,
            as_it_is,
            colours,
            alpha: prepared
                .alpha
                .as_deref()
                .map(|alpha| unpredict(&inflate(alpha), row, 1)),
        })
    }

    fn png_file(width: u32, height: u32, colour: png::ColorType, samples: &[u8]) -> Vec<u8> {
        let mut file = Vec::new();
        let mut encoder = png::Encoder::new(&mut file, width, height);
        encoder.set_color(colour);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(samples).unwrap();
        writer.finish().unwrap();
        file
    }

    #[test]
    fn a_png_is_its_pixels_with_its_transparency_apart() {
        // Two pixels: opaque red, and half-transparent blue.
        let file = png_file(
            2,
            1,
            png::ColorType::Rgba,
            &[255, 0, 0, 255, 0, 0, 255, 128],
        );
        let seen = seen(ImageKind::Png, file).unwrap();
        assert_eq!(seen.pixels, (2, 1));
        assert_eq!(seen.colour, Colour::Rgb);
        // Two pixels at 96 to the inch: a forty-eighth of an inch, or 1.5 points.
        assert_eq!(seen.size, (1.5, 0.75));
        assert_eq!(seen.colours.unwrap(), [255, 0, 0, 0, 0, 255]);
        assert_eq!(seen.alpha.unwrap(), [255, 128]);
    }

    #[test]
    fn an_opaque_picture_carries_no_mask() {
        let file = png_file(1, 1, png::ColorType::Rgba, &[1, 2, 3, 255]);
        let seen_rgba = seen(ImageKind::Png, file).unwrap();
        assert_eq!(seen_rgba.colours.unwrap(), [1, 2, 3]);
        assert!(seen_rgba.alpha.is_none());
        let grey = seen(
            ImageKind::Png,
            png_file(2, 1, png::ColorType::Grayscale, &[0, 200]),
        );
        let grey = grey.unwrap();
        assert_eq!(grey.colour, Colour::Grey);
        assert_eq!(grey.colours.unwrap(), [0, 200]);
        assert!(grey.alpha.is_none());
    }

    #[test]
    fn a_gif_is_its_first_frame() {
        // A 2×1 GIF: one red pixel and one transparent one, then a frame nobody sees.
        let mut file = Vec::new();
        {
            let palette = [255, 0, 0, 0, 0, 0];
            let mut encoder = gif::Encoder::new(&mut file, 2, 1, &palette).unwrap();
            let mut frame = gif::Frame {
                width: 2,
                height: 1,
                buffer: std::borrow::Cow::Borrowed(&[0, 1]),
                transparent: Some(1),
                ..gif::Frame::default()
            };
            encoder.write_frame(&frame).unwrap();
            frame.buffer = std::borrow::Cow::Borrowed(&[1, 1]);
            encoder.write_frame(&frame).unwrap();
        }
        let seen_gif = seen(ImageKind::Gif, file).unwrap();
        assert_eq!(seen_gif.pixels, (2, 1));
        assert_eq!(&seen_gif.colours.unwrap()[..3], [255, 0, 0]);
        assert_eq!(seen_gif.alpha.unwrap(), [255, 0]);

        // A 3×2 picture whose first frame is one blue pixel, in the bottom right corner.
        let mut file = Vec::new();
        {
            let palette = [0, 0, 255];
            let mut encoder = gif::Encoder::new(&mut file, 3, 2, &palette).unwrap();
            let frame = gif::Frame {
                left: 2,
                top: 1,
                width: 1,
                height: 1,
                buffer: std::borrow::Cow::Borrowed(&[0]),
                ..gif::Frame::default()
            };
            encoder.write_frame(&frame).unwrap();
        }
        let corner = seen(ImageKind::Gif, file).unwrap();
        assert_eq!(corner.pixels, (3, 2));
        assert_eq!(corner.alpha.unwrap(), [0, 0, 0, 0, 0, 255]);
        assert_eq!(&corner.colours.unwrap()[15..], [0, 0, 255]);
    }

    /// The smallest JPEG header that says what it is: start of image, then a frame header.
    fn jpeg_file(marker: u8, components: u8, adobe: bool) -> Vec<u8> {
        let mut file = vec![0xff, 0xd8];
        if adobe {
            file.extend([0xff, 0xee, 0x00, 0x0e]);
            file.extend(b"Adobe");
            file.extend([0x00, 0x64, 0x00, 0x00, 0x00, 0x00, 0x00]);
        }
        file.extend([
            0xff, marker, 0x00, 0x11, 8, 0x00, 0x02, 0x00, 0x03, components,
        ]);
        file.extend([0; 9]);
        file
    }

    #[test]
    fn a_jpeg_goes_in_as_it_is() {
        let file = jpeg_file(0xc0, 3, false);
        let seen_rgb = seen(ImageKind::Jpeg, file.clone()).unwrap();
        assert_eq!(seen_rgb.pixels, (3, 2));
        assert_eq!(seen_rgb.colour, Colour::Rgb);
        assert_eq!(seen_rgb.as_it_is, Some(file));
        assert!(seen_rgb.alpha.is_none());

        let progressive = seen(ImageKind::Jpeg, jpeg_file(0xc2, 1, false)).unwrap();
        assert_eq!(progressive.colour, Colour::Grey);
        let photoshop = seen(ImageKind::Jpeg, jpeg_file(0xc0, 4, true)).unwrap();
        assert_eq!(photoshop.colour, Colour::Cmyk { inverted: true });
        let cmyk = seen(ImageKind::Jpeg, jpeg_file(0xc0, 4, false)).unwrap();
        assert_eq!(cmyk.colour, Colour::Cmyk { inverted: false });
    }

    #[test]
    fn what_a_pdf_cannot_carry_is_left_to_its_description() {
        // An arithmetic-coded JPEG, a WebP, and a PNG cut off after its first bytes.
        assert!(seen(ImageKind::Jpeg, jpeg_file(0xc9, 3, false)).is_none());
        let mut webp = b"RIFF\0\0\0\0WEBPVP8 ".to_vec();
        webp.extend([0; 20]);
        assert!(seen(ImageKind::Webp, webp).is_none());
        let mut cut = png_file(4, 4, png::ColorType::Rgb, &[7; 48]);
        cut.truncate(40);
        assert!(seen(ImageKind::Png, cut).is_none());
    }
}
