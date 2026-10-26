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

/// Vanilla data nests a handful of levels; this bounds hostile inputs before
/// they exhaust the stack (each level costs a full parser frame).
const MAX_DEPTH: usize = 128;

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

    pub fn as_list_mut(&mut self) -> Option<&mut Vec<Tag>> {
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

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Tag::Byte(value) => Some(*value as f64),
            Tag::Short(value) => Some(*value as f64),
            Tag::Int(value) => Some(*value as f64),
            Tag::Long(value) => Some(*value as f64),
            Tag::Float(value) => Some(*value as f64),
            Tag::Double(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_i8_array(&self) -> Option<&[i8]> {
        match self {
            Tag::ByteArray(values) => Some(values),
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

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Tag> {
        match self {
            Tag::Compound(entries) => entries
                .iter_mut()
                .find(|(name, _)| name == key)
                .map(|(_, tag)| tag),
            _ => None,
        }
    }

    /// Insert or replace a compound entry, keeping existing order.
    pub fn set(&mut self, key: &str, value: Tag) {
        if let Tag::Compound(entries) = self {
            if let Some(entry) = entries.iter_mut().find(|(name, _)| name == key) {
                entry.1 = value;
            } else {
                entries.push((key.to_string(), value));
            }
        }
    }
}

fn tag_id(tag: &Tag) -> u8 {
    match tag {
        Tag::Byte(_) => BYTE,
        Tag::Short(_) => SHORT,
        Tag::Int(_) => INT,
        Tag::Long(_) => LONG,
        Tag::Float(_) => FLOAT,
        Tag::Double(_) => DOUBLE,
        Tag::ByteArray(_) => BYTE_ARRAY,
        Tag::String(_) => STRING,
        Tag::List(_) => LIST,
        Tag::Compound(_) => COMPOUND,
        Tag::IntArray(_) => INT_ARRAY,
        Tag::LongArray(_) => LONG_ARRAY,
    }
}

fn write_string(out: &mut Vec<u8>, value: &str) -> Result<(), Error> {
    let bytes = encode_modified_utf8(value);
    let len = u16::try_from(bytes.len()).map_err(|_| Error::StringTooLong(bytes.len()))?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&bytes);
    Ok(())
}

/// Java `DataOutput.writeUTF` modified UTF-8: NUL is `C0 80` and supplementary
/// characters are written as CESU-8 surrogate pairs.
fn encode_modified_utf8(value: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len());
    for unit in value.encode_utf16() {
        match unit {
            0x0001..=0x007F => out.push(unit as u8),
            0 => out.extend_from_slice(&[0xC0, 0x80]),
            0x0080..=0x07FF => {
                out.push(0xC0 | (unit >> 6) as u8);
                out.push(0x80 | (unit & 0x3F) as u8);
            }
            _ => {
                out.push(0xE0 | (unit >> 12) as u8);
                out.push(0x80 | ((unit >> 6) & 0x3F) as u8);
                out.push(0x80 | (unit & 0x3F) as u8);
            }
        }
    }
    out
}

/// Java `DataInput.readUTF` modified UTF-8.
///
/// Standard 4-byte UTF-8 is accepted as an extension: third-party tools and
/// older mcre builds write strings that way.
fn decode_modified_utf8(bytes: &[u8]) -> Result<String, Error> {
    fn continuation(byte: Option<&u8>) -> Result<u32, Error> {
        match byte {
            Some(byte) if byte & 0xC0 == 0x80 => Ok((byte & 0x3F) as u32),
            _ => Err(Error::InvalidUtf8),
        }
    }

    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let first = bytes[i];
        match first {
            0x01..=0x7F => {
                out.push(first as char);
                i += 1;
            }
            0x00 => return Err(Error::InvalidUtf8),
            first if first & 0xE0 == 0xC0 => {
                let value = (((first & 0x1F) as u32) << 6) | continuation(bytes.get(i + 1))?;
                if value == 0 {
                    out.push('\0');
                } else if value < 0x80 {
                    return Err(Error::InvalidUtf8);
                } else {
                    out.push(char::from_u32(value).ok_or(Error::InvalidUtf8)?);
                }
                i += 2;
            }
            first if first & 0xF0 == 0xE0 => {
                let value = (((first & 0x0F) as u32) << 12)
                    | (continuation(bytes.get(i + 1))? << 6)
                    | continuation(bytes.get(i + 2))?;
                if value < 0x800 {
                    return Err(Error::InvalidUtf8);
                }
                if (0xD800..=0xDBFF).contains(&value) {
                    let low_first = *bytes.get(i + 3).ok_or(Error::InvalidUtf8)?;
                    if low_first & 0xF0 != 0xE0 {
                        return Err(Error::InvalidUtf8);
                    }
                    let low = (((low_first & 0x0F) as u32) << 12)
                        | (continuation(bytes.get(i + 4))? << 6)
                        | continuation(bytes.get(i + 5))?;
                    if !(0xDC00..=0xDFFF).contains(&low) {
                        return Err(Error::InvalidUtf8);
                    }
                    let code = 0x10000 + ((value - 0xD800) << 10) + (low - 0xDC00);
                    out.push(char::from_u32(code).ok_or(Error::InvalidUtf8)?);
                    i += 6;
                } else if (0xDC00..=0xDFFF).contains(&value) {
                    return Err(Error::InvalidUtf8);
                } else {
                    out.push(char::from_u32(value).ok_or(Error::InvalidUtf8)?);
                    i += 3;
                }
            }
            first if first & 0xF8 == 0xF0 => {
                let value = (((first & 0x07) as u32) << 18)
                    | (continuation(bytes.get(i + 1))? << 12)
                    | (continuation(bytes.get(i + 2))? << 6)
                    | continuation(bytes.get(i + 3))?;
                if !(0x10000..=0x10FFFF).contains(&value) {
                    return Err(Error::InvalidUtf8);
                }
                out.push(char::from_u32(value).ok_or(Error::InvalidUtf8)?);
                i += 4;
            }
            _ => return Err(Error::InvalidUtf8),
        }
    }
    Ok(out)
}

fn write_payload(out: &mut Vec<u8>, tag: &Tag) -> Result<(), Error> {
    match tag {
        Tag::Byte(value) => out.push(*value as u8),
        Tag::Short(value) => out.extend_from_slice(&value.to_be_bytes()),
        Tag::Int(value) => out.extend_from_slice(&value.to_be_bytes()),
        Tag::Long(value) => out.extend_from_slice(&value.to_be_bytes()),
        Tag::Float(value) => out.extend_from_slice(&value.to_be_bytes()),
        Tag::Double(value) => out.extend_from_slice(&value.to_be_bytes()),
        Tag::ByteArray(values) => {
            out.extend_from_slice(&(values.len() as i32).to_be_bytes());
            out.extend(values.iter().map(|value| *value as u8));
        }
        Tag::String(value) => write_string(out, value)?,
        Tag::List(items) => {
            out.push(items.first().map_or(0, tag_id));
            out.extend_from_slice(&(items.len() as i32).to_be_bytes());
            for item in items {
                write_payload(out, item)?;
            }
        }
        Tag::Compound(entries) => {
            for (name, value) in entries {
                out.push(tag_id(value));
                write_string(out, name)?;
                write_payload(out, value)?;
            }
            out.push(0);
        }
        Tag::IntArray(values) => {
            out.extend_from_slice(&(values.len() as i32).to_be_bytes());
            for value in values {
                out.extend_from_slice(&value.to_be_bytes());
            }
        }
        Tag::LongArray(values) => {
            out.extend_from_slice(&(values.len() as i32).to_be_bytes());
            for value in values {
                out.extend_from_slice(&value.to_be_bytes());
            }
        }
    }
    Ok(())
}

/// Serialize a named root compound, the exact inverse of [`read_named`].
///
/// Strings use Java's modified UTF-8, so the output is readable by vanilla.
pub fn write_named(name: &str, tag: &Tag) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    out.push(COMPOUND);
    write_string(&mut out, name)?;
    write_payload(&mut out, tag)?;
    Ok(out)
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
        decode_modified_utf8(bytes)
    }

    fn skip_payload(&mut self, kind: u8, depth: usize) -> Result<(), Error> {
        if depth > MAX_DEPTH {
            return Err(Error::NbtTooDeep(MAX_DEPTH));
        }
        match kind {
            BYTE => {
                self.take(1)?;
            }
            SHORT => {
                self.take(2)?;
            }
            INT | FLOAT => {
                self.take(4)?;
            }
            LONG | DOUBLE => {
                self.take(8)?;
            }
            BYTE_ARRAY => {
                let len = self.i32()?;
                if len < 0 {
                    return Err(Error::NegativeLength);
                }
                self.take(len as usize)?;
            }
            STRING => {
                let len = self.u16()? as usize;
                self.take(len)?;
            }
            LIST => {
                let element_kind = self.u8()?;
                let len = self.i32()?;
                if len < 0 {
                    return Err(Error::NegativeLength);
                }
                for _ in 0..len {
                    self.skip_payload(element_kind, depth + 1)?;
                }
            }
            COMPOUND => loop {
                let kind = self.u8()?;
                if kind == 0 {
                    return Ok(());
                }
                self.name()?;
                self.skip_payload(kind, depth + 1)?;
            },
            INT_ARRAY => {
                let len = self.i32()?;
                if len < 0 {
                    return Err(Error::NegativeLength);
                }
                let bytes = (len as usize).checked_mul(4).ok_or(Error::Truncated)?;
                self.take(bytes)?;
            }
            LONG_ARRAY => {
                let len = self.i32()?;
                if len < 0 {
                    return Err(Error::NegativeLength);
                }
                let bytes = (len as usize).checked_mul(8).ok_or(Error::Truncated)?;
                self.take(bytes)?;
            }
            other => return Err(Error::UnknownTag(other)),
        }
        Ok(())
    }

    fn payload(&mut self, kind: u8, depth: usize) -> Result<Tag, Error> {
        if depth > MAX_DEPTH {
            return Err(Error::NbtTooDeep(MAX_DEPTH));
        }
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
                    items.push(self.payload(element_kind, depth + 1)?);
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
                    let value = self.payload(kind, depth + 1)?;
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
    let root = reader.payload(COMPOUND, 0)?;
    Ok((name, root))
}

/// Extract a top-level string entry without building the full tag tree.
///
/// Unknown payloads are skipped byte-wise, so this stays cheap even when the
/// target key sits in front of large compounds such as `sections`.
pub fn find_root_string(bytes: &[u8], key: &str) -> Result<Option<String>, Error> {
    let mut reader = Reader::new(bytes);
    let kind = reader.u8()?;
    if kind != COMPOUND {
        return Err(Error::ExpectedRootCompound(kind));
    }
    reader.name()?;
    loop {
        let kind = reader.u8()?;
        if kind == 0 {
            return Ok(None);
        }
        let name = reader.name()?;
        if name == key && kind == STRING {
            let len = reader.u16()? as usize;
            let value = reader.take(len)?;
            return Ok(Some(decode_modified_utf8(value)?));
        }
        reader.skip_payload(kind, 0)?;
    }
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
    fn write_round_trips_common_tags() {
        let (_, root) = read_named(&blob()).unwrap();
        let bytes = write_named("", &root).unwrap();
        let (name, reparsed) = read_named(&bytes).unwrap();
        assert_eq!(name, "");
        assert_eq!(root, reparsed);

        let named = write_named("Data", &root).unwrap();
        let (name, reparsed) = read_named(&named).unwrap();
        assert_eq!(name, "Data");
        assert_eq!(root, reparsed);
    }

    fn string_blob(key: &[u8], value: &[u8]) -> Vec<u8> {
        let mut out = vec![COMPOUND, 0, 0, STRING];
        out.extend_from_slice(&(key.len() as u16).to_be_bytes());
        out.extend_from_slice(key);
        out.extend_from_slice(&(value.len() as u16).to_be_bytes());
        out.extend_from_slice(value);
        out.push(0);
        out
    }

    #[test]
    fn decodes_modified_utf8_strings() {
        let cases: [(&[u8], &str); 4] = [
            (b"hello", "hello"),
            ("é日本語".as_bytes(), "é日本語"),
            (&[0xC0, 0x80], "\0"),
            (&[0xED, 0xA0, 0xBE, 0xED, 0xB6, 0x80], "🦀"),
        ];
        for (bytes, expected) in cases {
            let (_, root) = read_named(&string_blob(b"value", bytes)).unwrap();
            assert_eq!(root.get("value"), Some(&Tag::String(expected.into())));
        }
    }

    #[test]
    fn decodes_standard_utf8_extension() {
        let (_, root) = read_named(&string_blob(b"value", "🦀".as_bytes())).unwrap();
        assert_eq!(root.get("value"), Some(&Tag::String("🦀".into())));
    }

    #[test]
    fn rejects_malformed_modified_utf8() {
        for bytes in [
            &[0xED, 0xA0, 0xBE][..],
            &[0xED, 0xBE, 0x80][..],
            &[0xED, 0xA0, 0xBE, 0x8D, 0xB6, 0x80][..],
            &[0xC0, 0x41][..],
            &[0xC1, 0x81][..],
            &[0xE0, 0x80, 0x80][..],
            &[0xF0, 0x9F, 0xA6][..],
            &[0xF8, 0x88, 0x80, 0x80, 0x80][..],
            &[0x00][..],
        ] {
            assert_eq!(
                read_named(&string_blob(b"value", bytes)).unwrap_err(),
                Error::InvalidUtf8,
                "{bytes:02X?}"
            );
        }
    }

    #[test]
    fn writes_modified_utf8_and_round_trips() {
        assert_eq!(encode_modified_utf8("\0"), [0xC0, 0x80]);
        assert_eq!(
            encode_modified_utf8("🦀"),
            [0xED, 0xA0, 0xBE, 0xED, 0xB6, 0x80]
        );
        let text = "a\0b é日本語 🦀";
        let root = Tag::Compound(vec![("text".into(), Tag::String(text.into()))]);
        let bytes = write_named("", &root).unwrap();
        let (_, reparsed) = read_named(&bytes).unwrap();
        assert_eq!(reparsed, root);
    }

    #[test]
    fn write_rejects_strings_over_u16_bytes() {
        let long = "a".repeat(u16::MAX as usize + 1);
        assert_eq!(
            write_named("", &Tag::Compound(vec![("long".into(), Tag::String(long))]),).unwrap_err(),
            Error::StringTooLong(u16::MAX as usize + 1)
        );
    }

    #[test]
    fn find_root_string_decodes_modified_utf8() {
        let mut blob = vec![COMPOUND, 0, 0];
        blob.push(STRING);
        blob.extend_from_slice(&[0, 4]);
        blob.extend_from_slice(b"name");
        blob.extend_from_slice(&[0, 6]);
        blob.extend_from_slice(&[0xED, 0xA0, 0xBE, 0xED, 0xB6, 0x80]);
        blob.push(0);
        assert_eq!(
            find_root_string(&blob, "name").unwrap(),
            Some("🦀".to_string())
        );
    }

    #[test]
    fn set_replaces_and_appends() {
        let mut root = Tag::Compound(vec![
            ("a".to_string(), Tag::Int(1)),
            ("b".to_string(), Tag::Int(2)),
        ]);
        root.set("a", Tag::Byte(9));
        root.set("c", Tag::Long(3));
        assert_eq!(root.get("a"), Some(&Tag::Byte(9)));
        assert_eq!(root.get("b"), Some(&Tag::Int(2)));
        assert_eq!(root.get("c"), Some(&Tag::Long(3)));
        let Tag::Compound(entries) = &root else {
            panic!("compound");
        };
        assert_eq!(
            entries
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
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

    fn string_entry(out: &mut Vec<u8>, key: &str, value: &str) {
        out.push(STRING);
        out.extend_from_slice(&(key.len() as u16).to_be_bytes());
        out.extend_from_slice(key.as_bytes());
        out.extend_from_slice(&(value.len() as u16).to_be_bytes());
        out.extend_from_slice(value.as_bytes());
    }

    fn compound_entry(out: &mut Vec<u8>, key: &str, inner: &[u8]) {
        out.push(COMPOUND);
        out.extend_from_slice(&(key.len() as u16).to_be_bytes());
        out.extend_from_slice(key.as_bytes());
        out.extend_from_slice(inner);
        out.push(0);
    }

    fn list_entry(out: &mut Vec<u8>, key: &str, elements: &[i32]) {
        out.push(LIST);
        out.extend_from_slice(&(key.len() as u16).to_be_bytes());
        out.extend_from_slice(key.as_bytes());
        out.push(INT);
        out.extend_from_slice(&(elements.len() as i32).to_be_bytes());
        for element in elements {
            out.extend_from_slice(&element.to_be_bytes());
        }
    }

    fn array_entries() -> Vec<u8> {
        let mut out = vec![COMPOUND, 0, 0];
        out.push(BYTE_ARRAY);
        out.extend_from_slice(&[0, 2]);
        out.extend_from_slice(b"xy");
        out.extend_from_slice(&2i32.to_be_bytes());
        out.extend_from_slice(&[9, 10]);
        out.extend_from_slice(&[3, 0, 1, b'i']);
        out.extend_from_slice(&7i32.to_be_bytes());
        out.extend_from_slice(&[4, 0, 1, b'l']);
        out.extend_from_slice(&(-9i64).to_be_bytes());
        out
    }

    #[test]
    fn finds_strings_after_skipped_entries() {
        let mut blob = array_entries();
        list_entry(&mut blob, "ticks", &[1, 2, 3]);
        compound_entry(&mut blob, "sections", &{
            let mut inner = Vec::new();
            string_entry(&mut inner, "block", "stone");
            inner
        });
        string_entry(&mut blob, "status", "minecraft:full");
        blob.push(0);

        assert_eq!(
            find_root_string(&blob, "status").unwrap(),
            Some("minecraft:full".to_string())
        );
        assert_eq!(find_root_string(&blob, "missing").unwrap(), None);
        assert_eq!(
            find_root_string(&blob, "block").unwrap(),
            None,
            "nested keys must not match"
        );
    }

    #[test]
    fn finds_strings_before_skipped_entries() {
        let mut blob = vec![COMPOUND, 0, 0];
        string_entry(&mut blob, "status", "minecraft:features");
        compound_entry(&mut blob, "sections", &[]);
        blob.push(0);
        assert_eq!(
            find_root_string(&blob, "status").unwrap(),
            Some("minecraft:features".to_string())
        );
    }

    #[test]
    fn rejects_excessive_nesting() {
        let depth = MAX_DEPTH + 1;
        let mut blob = vec![COMPOUND, 0, 0];
        for _ in 0..depth {
            blob.push(COMPOUND);
            blob.extend_from_slice(&[0, 1, b'x']);
        }
        blob.extend(std::iter::repeat_n(0u8, depth));
        assert_eq!(read_named(&blob).unwrap_err(), Error::NbtTooDeep(MAX_DEPTH));
    }
}
