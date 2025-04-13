use binrw::{
    io::{Read, Seek},
    BinRead, BinReaderExt,
};

use super::NiLight;

#[derive(Debug, PartialEq, BinRead)]
pub struct NiPointLight {
    pub base: NiLight,
    pub constant_attenuation: f32,
    pub linear_attenuation: f32,
    pub quadratic_attenuation: f32,
}

impl NiPointLight {
    pub fn parse<R: Read + Seek>(reader: &mut R) -> anyhow::Result<Self> {
        Ok(reader.read_le()?)
    }
}

impl std::ops::Deref for NiPointLight {
    type Target = NiLight;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}
