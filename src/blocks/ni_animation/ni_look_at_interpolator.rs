use binrw::{
    io::{Read, Seek},
    BinRead, BinReaderExt,
};

use crate::{
    blocks::NiString,
    common::{BlockRef, NiQuatTransform},
};

use super::NiInterpolator;

#[derive(Debug, PartialEq, BinRead)]
pub struct NiLookAtInterpolator {
    pub base: NiInterpolator,
    pub flags: u16,
    pub look_at: BlockRef,
    pub look_at_name: NiString,
    pub transform: NiQuatTransform,
    pub interpolator_translation: BlockRef,
    pub interpolator_roll: BlockRef,
    pub interpolator_scale: BlockRef,
}

impl NiLookAtInterpolator {
    pub fn parse<R: Read + Seek>(reader: &mut R) -> anyhow::Result<Self> {
        Ok(reader.read_le()?)
    }
}
