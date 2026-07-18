use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum Fo3Error {
    #[error("unexpected end of NIF while reading {field} at byte {offset}")]
    UnexpectedEof { field: &'static str, offset: usize },
    #[error("invalid FO3/FNV NIF header string: {0:?}")]
    InvalidHeaderString(String),
    #[error("unsupported NIF version 0x{0:08x}; expected 20.2.0.7")]
    UnsupportedVersion(u32),
    #[error("unsupported NIF endian marker {0}; only little-endian PC assets are supported")]
    UnsupportedEndian(u8),
    #[error("unsupported NIF user version {0}; expected 11")]
    UnsupportedUserVersion(u32),
    #[error("unsupported FO3/FNV Bethesda stream version {0}")]
    UnsupportedBethesdaVersion(u32),
    #[error("{field} count {count} exceeds safety limit {limit}")]
    CountLimit {
        field: &'static str,
        count: usize,
        limit: usize,
    },
    #[error("invalid UTF-8 in {field} at byte {offset}")]
    InvalidUtf8 { field: &'static str, offset: usize },
    #[error("block {block} uses missing block type index {type_index} (type count {type_count})")]
    InvalidBlockTypeIndex {
        block: usize,
        type_index: usize,
        type_count: usize,
    },
    #[error("block payloads require {required} bytes but only {available} remain")]
    BlockPayloadOutOfBounds { required: usize, available: usize },
    #[error("footer root {root_index} references invalid block {block_index} (block count {block_count})")]
    InvalidRootReference {
        root_index: usize,
        block_index: i32,
        block_count: usize,
    },
    #[error("NIF has {0} unparsed trailing bytes after its footer")]
    TrailingBytes(usize),
    #[error("block {block} ({type_name}) has {remaining} unparsed bytes")]
    UnparsedBlockBytes {
        block: usize,
        type_name: String,
        remaining: usize,
    },
    #[error("block {block} ({type_name}) references invalid string index {string_index} (string count {string_count})")]
    InvalidStringIndex {
        block: usize,
        type_name: String,
        string_index: i32,
        string_count: usize,
    },
    #[error("block {block} ({type_name}) field {field} count {count} cannot fit in {remaining} remaining bytes")]
    InvalidBlockCount {
        block: usize,
        type_name: String,
        field: &'static str,
        count: usize,
        remaining: usize,
    },
    #[error("integer overflow while calculating {0}")]
    Overflow(&'static str),
}
