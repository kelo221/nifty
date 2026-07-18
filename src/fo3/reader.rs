use super::Fo3Error;

pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(crate) fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.offset)
    }

    pub(crate) fn read_u8(&mut self, field: &'static str) -> Result<u8, Fo3Error> {
        Ok(self.take(1, field)?[0])
    }

    pub(crate) fn read_u16(&mut self, field: &'static str) -> Result<u16, Fo3Error> {
        Ok(u16::from_le_bytes(
            self.take(2, field)?.try_into().expect("length checked"),
        ))
    }

    pub(crate) fn read_u32(&mut self, field: &'static str) -> Result<u32, Fo3Error> {
        Ok(u32::from_le_bytes(
            self.take(4, field)?.try_into().expect("length checked"),
        ))
    }

    pub(crate) fn read_i32(&mut self, field: &'static str) -> Result<i32, Fo3Error> {
        Ok(i32::from_le_bytes(
            self.take(4, field)?.try_into().expect("length checked"),
        ))
    }

    pub(crate) fn read_f32(&mut self, field: &'static str) -> Result<f32, Fo3Error> {
        Ok(f32::from_le_bytes(
            self.take(4, field)?.try_into().expect("length checked"),
        ))
    }

    pub(crate) fn read_line(&mut self, field: &'static str) -> Result<String, Fo3Error> {
        let start = self.offset;
        let relative_end = self.bytes[start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or(Fo3Error::UnexpectedEof {
                field,
                offset: start,
            })?;
        let end = start + relative_end;
        let value = std::str::from_utf8(&self.bytes[start..end])
            .map_err(|_| Fo3Error::InvalidUtf8 {
                field,
                offset: start,
            })?
            .to_owned();
        self.offset = end + 1;
        Ok(value)
    }

    pub(crate) fn read_short_string(&mut self, field: &'static str) -> Result<String, Fo3Error> {
        let length = self.read_u8(field)? as usize;
        let mut value = self.read_string_bytes(length, field)?;
        if value.ends_with('\0') {
            value.pop();
        }
        Ok(value)
    }

    pub(crate) fn read_sized_string(&mut self, field: &'static str) -> Result<String, Fo3Error> {
        let length = self.read_u32(field)? as usize;
        self.read_string_bytes(length, field)
    }

    pub(crate) fn take(
        &mut self,
        length: usize,
        field: &'static str,
    ) -> Result<&'a [u8], Fo3Error> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(Fo3Error::Overflow(field))?;
        if end > self.bytes.len() {
            return Err(Fo3Error::UnexpectedEof {
                field,
                offset: self.offset,
            });
        }
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }

    fn read_string_bytes(
        &mut self,
        length: usize,
        field: &'static str,
    ) -> Result<String, Fo3Error> {
        let start = self.offset;
        std::str::from_utf8(self.take(length, field)?)
            .map(str::to_owned)
            .map_err(|_| Fo3Error::InvalidUtf8 {
                field,
                offset: start,
            })
    }
}
