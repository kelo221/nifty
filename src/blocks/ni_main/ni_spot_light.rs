use binrw::{
    io::{Read, Seek},
    BinRead, BinReaderExt,
};

use super::NiPointLight;

#[derive(Debug, PartialEq, BinRead)]
pub struct NiSpotLight {
    pub base: NiPointLight,
    pub outer_spot_angle: f32,
    pub exponent: f32,
}

impl NiSpotLight {
    pub fn parse<R: Read + Seek>(reader: &mut R) -> anyhow::Result<Self> {
        Ok(reader.read_le()?)
    }
}

impl std::ops::Deref for NiSpotLight {
    type Target = NiPointLight;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}
