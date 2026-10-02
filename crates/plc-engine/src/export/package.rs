//! The container a Word or OpenDocument file is: a zip of XML parts and pictures.
//!
//! Every part carries the same fixed date, so the same document always makes the same bytes —
//! which is what lets the tests compare an export exactly, and what keeps two exports of an
//! unchanged document from looking different to a backup.

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

/// One file inside the package.
pub struct Part {
    pub name: String,
    pub bytes: Vec<u8>,
    /// Stored as it is rather than compressed: OpenDocument's `mimetype` must be, so a reader
    /// can recognise the file from its first bytes, and a picture gains nothing from it.
    pub stored: bool,
}

impl Part {
    pub fn xml(name: &str, xml: String) -> Self {
        Part {
            name: name.to_string(),
            bytes: xml.into_bytes(),
            stored: false,
        }
    }

    pub fn stored(name: &str, bytes: Vec<u8>) -> Self {
        Part {
            name: name.to_string(),
            bytes,
            stored: true,
        }
    }
}

/// The parts, zipped in the order given.
pub fn pack(parts: &[Part]) -> zip::result::ZipResult<Vec<u8>> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for part in parts {
        let method = if part.stored {
            CompressionMethod::Stored
        } else {
            CompressionMethod::Deflated
        };
        let options = SimpleFileOptions::default()
            .compression_method(method)
            .last_modified_time(DateTime::default());
        zip.start_file(part.name.as_str(), options)?;
        zip.write_all(&part.bytes)?;
    }
    Ok(zip.finish()?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn parts_come_back_in_order_and_as_they_went_in() {
        let bytes = pack(&[
            Part::stored(
                "mimetype",
                b"application/vnd.oasis.opendocument.text".to_vec(),
            ),
            Part::xml("content.xml", "<a>Белок</a>".to_string()),
        ])
        .unwrap();
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut first = zip.by_index(0).unwrap();
        assert_eq!(first.name(), "mimetype");
        assert_eq!(first.compression(), CompressionMethod::Stored);
        let mut text = String::new();
        first.read_to_string(&mut text).unwrap();
        assert_eq!(text, "application/vnd.oasis.opendocument.text");
        drop(first);
        let mut second = zip.by_name("content.xml").unwrap();
        assert_eq!(second.compression(), CompressionMethod::Deflated);
        let mut text = String::new();
        second.read_to_string(&mut text).unwrap();
        assert_eq!(text, "<a>Белок</a>");
    }

    #[test]
    fn the_same_parts_make_the_same_bytes() {
        let make = || pack(&[Part::xml("a.xml", "<a/>".to_string())]).unwrap();
        assert_eq!(make(), make());
    }
}
