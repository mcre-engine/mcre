use crate::Error;

pub const BYTE: u8 = 1;
pub const SHORT: u8 = 2;
pub const INT: u8 = 3;
pub const LONG: u8 = 4;
pub const FLOAT: u8 = 5;
pub const DOUBLE: u8 = 6;
pub const BYTE_ARRAY: u8 = 7;
pub const STRING: u8 = 8;
pub const LIST: u8 = 9;
pub const COMPOUND: u8 = 10;
pub const INT_ARRAY: u8 = 11;
pub const LONG_ARRAY: u8 = 12;

#[derive(Debug, Clone, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    String(String),
    List(Vec<Tag>),
    Compound(Vec<(String, Tag)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

impl Tag {
    pub fn get(&self, key: &str) -> Option<&Tag> {
        match self {
            Tag::Compound(entries) => entries
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, tag)| tag),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Tag::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_compound(&self) -> Option<&[(String, Tag)]> {
        match self {
            Tag::Compound(entries) => Some(entries),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Tag]> {
        match self {
            Tag::List(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_i32(&self) -> Option<i32> {
        match self {
            Tag::Byte(value) => Some(*value as i32),
            Tag::Short(value) => Some(*value as i32),
            Tag::Int(value) => Some(*value),
            Tag::Long(value) => i32::try_from(*value).ok(),
            _ => None,
        }
    }

    pub fn as_i64_array(&self) -> Option<Vec<i64>> {
        match self {
            Tag::LongArray(values) => Some(values.clone()),
            Tag::IntArray(values) => Some(values.iter().map(|value| *value as i64).collect()),
            _ => None,
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, len: usize) -> Result<&'a [u8], Error> {
        let end = self.offset.checked_add(len).ok_or(Error::Truncated)?;
        if end > self.bytes.len() {
            return Err(Error::Truncated);
        }
        let slice = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, Error> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn i16(&mut self) -> Result<i16, Error> {
        let bytes = self.take(2)?;
        Ok(i16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn i32(&mut self) -> Result<i32, Error> {
        let bytes = self.take(4)?;
        Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn i64(&mut self) -> Result<i64, Error> {
        let bytes = self.take(8)?;
        Ok(i64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn f32(&mut self) -> Result<f32, Error> {
        let bytes = self.take(4)?;
        Ok(f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn f64(&mut self) -> Result<f64, Error> {
        let bytes = self.take(8)?;
        Ok(f64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn name(&mut self) -> Result<String, Error> {
        let len = self.u16()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| Error::InvalidUtf8)
    }

    fn payload(&mut self, kind: u8) -> Result<Tag, Error> {
        match kind {
            BYTE => Ok(Tag::Byte(self.take(1)?[0] as i8)),
            SHORT => Ok(Tag::Short(self.i16()?)),
            INT => Ok(Tag::Int(self.i32()?)),
            LONG => Ok(Tag::Long(self.i64()?)),
            FLOAT => Ok(Tag::Float(self.f32()?)),
            DOUBLE => Ok(Tag::Double(self.f64()?)),
            BYTE_ARRAY => {
                let len = self.i32()?;
                if len < 0 {
                    return Err(Error::NegativeLength);
                }
                let bytes = self.take(len as usize)?;
                Ok(Tag::ByteArray(
                    bytes.iter().map(|byte| *byte as i8).collect(),
                ))
            }
            STRING => Ok(Tag::String(self.name()?)),
            LIST => {
                let element_kind = self.u8()?;
                let len = self.i32()?;
                if len < 0 {
                    return Err(Error::NegativeLength);
                }
                let mut items = Vec::with_capacity(len.min(4096) as usize);
                for _ in 0..len {
                    items.push(self.payload(element_kind)?);
                }
                Ok(Tag::List(items))
            }
            COMPOUND => {
                let mut entries = Vec::new();
                loop {
                    let kind = self.u8()?;
                    if kind == 0 {
                        return Ok(Tag::Compound(entries));
                    }
                    let name = self.name()?;
                    let value = self.payload(kind)?;
                    entries.push((name, value));
                }
            }
            INT_ARRAY => {
                let len = self.i32()?;
                if len < 0 {
                    return Err(Error::NegativeLength);
                }
                let mut values = Vec::with_capacity(len.min(4096) as usize);
                for _ in 0..len {
                    values.push(self.i32()?);
                }
                Ok(Tag::IntArray(values))
            }
            LONG_ARRAY => {
                let len = self.i32()?;
                if len < 0 {
                    return Err(Error::NegativeLength);
                }
                let mut values = Vec::with_capacity(len.min(4096) as usize);
                for _ in 0..len {
                    values.push(self.i64()?);
                }
                Ok(Tag::LongArray(values))
            }
            other => Err(Error::UnknownTag(other)),
        }
    }
}

pub fn read_named(bytes: &[u8]) -> Result<(String, Tag), Error> {
    let mut reader = Reader::new(bytes);
    let kind = reader.u8()?;
    if kind != COMPOUND {
        return Err(Error::ExpectedRootCompound(kind));
    }
    let name = reader.name()?;
    let root = reader.payload(COMPOUND)?;
    Ok((name, root))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blob() -> Vec<u8> {
        let mut out = Vec::new();
        out.push(COMPOUND);
        out.extend_from_slice(&[0, 0]);
        out.push(INT);
        out.extend_from_slice(&[0, 3]);
        out.extend_from_slice(b"pos");
        out.extend_from_slice(&42i32.to_be_bytes());
        out.push(STRING);
        out.extend_from_slice(&[0, 4]);
        out.extend_from_slice(b"name");
        out.extend_from_slice(&[0, 5]);
        out.extend_from_slice(b"value");
        out.push(LIST);
        out.extend_from_slice(&[0, 4]);
        out.extend_from_slice(b"list");
        out.push(INT);
        out.extend_from_slice(&2i32.to_be_bytes());
        out.extend_from_slice(&7i32.to_be_bytes());
        out.extend_from_slice(&(-3i32).to_be_bytes());
        out.push(COMPOUND);
        out.extend_from_slice(&[0, 5]);
        out.extend_from_slice(b"inner");
        out.push(BYTE);
        out.extend_from_slice(&[0, 1]);
        out.extend_from_slice(b"b");
        out.push(0xFB);
        out.push(0);
        out.push(0);
        out
    }

    #[test]
    fn parses_all_common_tags() {
        let (name, root) = read_named(&blob()).unwrap();
        assert_eq!(name, "");
        assert_eq!(root.get("pos"), Some(&Tag::Int(42)));
        assert_eq!(root.get("name"), Some(&Tag::String("value".to_string())));
        assert_eq!(
            root.get("list"),
            Some(&Tag::List(vec![Tag::Int(7), Tag::Int(-3)]))
        );
        let inner = root.get("inner").unwrap();
        assert_eq!(inner.get("b"), Some(&Tag::Byte(-5)));
    }

    #[test]
    fn rejects_truncated_and_unknown() {
        assert_eq!(read_named(&[10, 0]).unwrap_err(), Error::Truncated);
        assert_eq!(read_named(&[10, 0, 0, 1]).unwrap_err(), Error::Truncated);
        let mut with_bad_list = vec![COMPOUND, 0, 0, LIST, 0, 1, b'l', 99];
        with_bad_list.extend_from_slice(&1i32.to_be_bytes());
        assert_eq!(
            read_named(&with_bad_list).unwrap_err(),
            Error::UnknownTag(99)
        );
        assert_eq!(
            read_named(&[3, 0, 0]).unwrap_err(),
            Error::ExpectedRootCompound(3)
        );
    }
}
