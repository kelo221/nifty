use binrw::{
    io::{Read, Seek},
    BinRead, BinReaderExt,
};

use super::NiLight;

#[derive(Debug, PartialEq, BinRead)]
pub struct NiAmbientLight {
    pub base: NiLight,
}

impl NiAmbientLight {
    pub fn parse<R: Read + Seek>(reader: &mut R) -> anyhow::Result<Self> {
        Ok(reader.read_le()?)
    }
}

impl std::ops::Deref for NiAmbientLight {
    type Target = NiLight;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}
