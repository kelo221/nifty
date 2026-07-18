//! Focused Fallout 3 / Fallout: New Vegas NIF support.
//!
//! This is intentionally isolated from the crate's legacy 20.0.0.4 parser.
//! The layouts are derived from the GPL-licensed NifTools `nifxml` 20.2.0.7
//! schema and checked against FO3/FNV assets. No Bethesda data is embedded.

mod error;
#[cfg(feature = "fo3_glb")]
mod glb;
mod physics;
mod reader;
mod scene;
mod typed;

pub use error::Fo3Error;
#[cfg(feature = "fo3_glb")]
pub use glb::*;
pub use physics::*;
pub use scene::*;
pub use typed::*;

use reader::Reader;

pub const FILE_VERSION: u32 = 0x1402_0007;
pub const USER_VERSION: u32 = 11;

const HEADER_TEXT: &str = "Gamebryo File Format, Version 20.2.0.7";
const MAX_BLOCKS: usize = 1_000_000;
const MAX_BLOCK_TYPES: usize = u16::MAX as usize;
const MAX_STRINGS: usize = 1_000_000;
const MAX_GROUPS: usize = 1_000_000;
const MAX_ROOTS: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BethesdaHeader {
    pub version: u32,
    pub author: String,
    pub process_script: String,
    pub export_script: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub header_string: String,
    pub version: u32,
    pub user_version: u32,
    pub block_type_names: Vec<String>,
    pub block_type_indices: Vec<u16>,
    pub block_sizes: Vec<u32>,
    pub strings: Vec<String>,
    pub groups: Vec<u32>,
    pub bethesda: BethesdaHeader,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawBlock {
    pub index: u32,
    pub type_name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub header: Header,
    pub blocks: Vec<RawBlock>,
    pub roots: Vec<i32>,
}

pub fn parse(bytes: &[u8]) -> Result<Document, Fo3Error> {
    let mut reader = Reader::new(bytes);
    let header_string = reader.read_line("header string")?;
    if header_string != HEADER_TEXT {
        return Err(Fo3Error::InvalidHeaderString(header_string));
    }

    let version = reader.read_u32("version")?;
    if version != FILE_VERSION {
        return Err(Fo3Error::UnsupportedVersion(version));
    }
    let endian = reader.read_u8("endian marker")?;
    if endian != 1 {
        return Err(Fo3Error::UnsupportedEndian(endian));
    }
    let user_version = reader.read_u32("user version")?;
    if user_version != USER_VERSION {
        return Err(Fo3Error::UnsupportedUserVersion(user_version));
    }
    let block_count = checked_count(
        reader.read_u32("block count")? as usize,
        MAX_BLOCKS,
        "block",
    )?;

    let bethesda_version = reader.read_u32("Bethesda stream version")?;
    if !is_supported_bethesda_version(bethesda_version) {
        return Err(Fo3Error::UnsupportedBethesdaVersion(bethesda_version));
    }
    let bethesda = BethesdaHeader {
        version: bethesda_version,
        author: reader.read_short_string("author")?,
        process_script: reader.read_short_string("process script")?,
        export_script: reader.read_short_string("export script")?,
    };

    let block_type_count = checked_count(
        reader.read_u16("block type count")? as usize,
        MAX_BLOCK_TYPES,
        "block type",
    )?;
    let mut block_type_names = Vec::with_capacity(block_type_count);
    for _ in 0..block_type_count {
        block_type_names.push(reader.read_sized_string("block type name")?);
    }

    let mut block_type_indices = Vec::with_capacity(block_count);
    for block in 0..block_count {
        let type_index = reader.read_u16("block type index")?;
        if type_index as usize >= block_type_count {
            return Err(Fo3Error::InvalidBlockTypeIndex {
                block,
                type_index: type_index as usize,
                type_count: block_type_count,
            });
        }
        block_type_indices.push(type_index);
    }

    let mut block_sizes = Vec::with_capacity(block_count);
    let mut total_block_bytes = 0usize;
    for _ in 0..block_count {
        let size = reader.read_u32("block size")?;
        total_block_bytes = total_block_bytes
            .checked_add(size as usize)
            .ok_or(Fo3Error::Overflow("block payload size"))?;
        block_sizes.push(size);
    }

    let string_count = checked_count(
        reader.read_u32("string count")? as usize,
        MAX_STRINGS,
        "string",
    )?;
    let declared_max_string_length = reader.read_u32("maximum string length")? as usize;
    let mut strings = Vec::with_capacity(string_count);
    for _ in 0..string_count {
        let value = reader.read_sized_string("string table value")?;
        if value.len() > declared_max_string_length {
            return Err(Fo3Error::CountLimit {
                field: "string length",
                count: value.len(),
                limit: declared_max_string_length,
            });
        }
        strings.push(value);
    }

    let group_count = checked_count(
        reader.read_u32("group count")? as usize,
        MAX_GROUPS,
        "group",
    )?;
    let mut groups = Vec::with_capacity(group_count);
    for _ in 0..group_count {
        groups.push(reader.read_u32("group")?);
    }

    if total_block_bytes > reader.remaining() {
        return Err(Fo3Error::BlockPayloadOutOfBounds {
            required: total_block_bytes,
            available: reader.remaining(),
        });
    }
    let mut blocks = Vec::with_capacity(block_count);
    for (index, (&type_index, &size)) in block_type_indices.iter().zip(&block_sizes).enumerate() {
        blocks.push(RawBlock {
            index: index as u32,
            type_name: block_type_names[type_index as usize].clone(),
            bytes: reader.take(size as usize, "block payload")?.to_vec(),
        });
    }

    let root_count = checked_count(
        reader.read_u32("footer root count")? as usize,
        MAX_ROOTS,
        "footer root",
    )?;
    let mut roots = Vec::with_capacity(root_count);
    for root_index in 0..root_count {
        let block_index = reader.read_i32("footer root")?;
        if block_index < -1 || block_index as usize >= block_count {
            return Err(Fo3Error::InvalidRootReference {
                root_index,
                block_index,
                block_count,
            });
        }
        roots.push(block_index);
    }
    if reader.remaining() != 0 {
        return Err(Fo3Error::TrailingBytes(reader.remaining()));
    }

    Ok(Document {
        header: Header {
            header_string,
            version,
            user_version,
            block_type_names,
            block_type_indices,
            block_sizes,
            strings,
            groups,
            bethesda,
        },
        blocks,
        roots,
    })
}

pub fn is_supported_bethesda_version(version: u32) -> bool {
    matches!(
        version,
        14 | 16 | 21 | 24 | 25 | 26 | 27 | 28 | 30 | 31 | 32 | 33 | 34
    )
}

fn checked_count(count: usize, limit: usize, field: &'static str) -> Result<usize, Fo3Error> {
    if count > limit {
        Err(Fo3Error::CountLimit {
            field,
            count,
            limit,
        })
    } else {
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_sized_string(bytes: &mut Vec<u8>, value: &str) {
        bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }

    fn synthetic_nif(block_type_index: u16, trailing: &[u8]) -> Vec<u8> {
        let block = [1_u8, 2, 3, 4];
        let mut bytes = Vec::new();
        bytes.extend_from_slice(HEADER_TEXT.as_bytes());
        bytes.push(b'\n');
        bytes.extend_from_slice(&FILE_VERSION.to_le_bytes());
        bytes.push(1);
        bytes.extend_from_slice(&USER_VERSION.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&34_u32.to_le_bytes());
        for value in ["test", "process", "export"] {
            bytes.push(value.len() as u8);
            bytes.extend_from_slice(value.as_bytes());
        }
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        push_sized_string(&mut bytes, "NiNode");
        bytes.extend_from_slice(&block_type_index.to_le_bytes());
        bytes.extend_from_slice(&(block.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&4_u32.to_le_bytes());
        push_sized_string(&mut bytes, "Root");
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&block);
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.extend_from_slice(&0_i32.to_le_bytes());
        bytes.extend_from_slice(trailing);
        bytes
    }

    #[test]
    fn parses_size_bounded_fo3_document() {
        let document = parse(&synthetic_nif(0, &[])).unwrap();
        assert_eq!(document.header.version, FILE_VERSION);
        assert_eq!(document.header.user_version, USER_VERSION);
        assert_eq!(document.header.bethesda.version, 34);
        assert_eq!(document.header.strings, ["Root"]);
        assert_eq!(document.blocks.len(), 1);
        assert_eq!(document.blocks[0].type_name, "NiNode");
        assert_eq!(document.blocks[0].bytes, [1, 2, 3, 4]);
        assert_eq!(document.roots, [0]);
    }

    #[test]
    fn rejects_invalid_type_index_before_reading_payload() {
        assert_eq!(
            parse(&synthetic_nif(1, &[])),
            Err(Fo3Error::InvalidBlockTypeIndex {
                block: 0,
                type_index: 1,
                type_count: 1,
            })
        );
    }

    #[test]
    fn rejects_trailing_bytes() {
        assert_eq!(
            parse(&synthetic_nif(0, &[9, 9])),
            Err(Fo3Error::TrailingBytes(2))
        );
    }

    #[test]
    fn pins_known_fo3_fonv_stream_versions() {
        for version in [14, 16, 21, 24, 25, 26, 27, 28, 30, 31, 32, 33, 34] {
            assert!(is_supported_bethesda_version(version));
        }
        for version in [0, 11, 22, 29, 35, 83, 100, 130] {
            assert!(!is_supported_bethesda_version(version));
        }
    }
}
