//! Reading a binary property list, and a keyed archive inside one.
//!
//! Read only, for the few system files a simulator keeps on this Mac.

use std::collections::BTreeMap;

/// A value in a property list.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Real(f64),
    /// Seconds since 2001-01-01 UTC.
    Date(f64),
    Data(Vec<u8>),
    Str(String),
    /// A reference into a keyed archive's `$objects`.
    Uid(usize),
    Array(Vec<Value>),
    Dict(BTreeMap<String, Value>),
}

impl Value {
    pub(crate) fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Dict(d) => d.get(key),
            _ => None,
        }
    }

    pub(crate) fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub(crate) fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub(crate) fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }
}

/// Objects nest this deep at most; a file that goes deeper is a cycle.
const DEEPEST: usize = 64;

struct Reader<'a> {
    bytes: &'a [u8],
    offsets: Vec<usize>,
    ref_size: usize,
}

fn be(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0, |n, b| (n << 8) | u64::from(*b))
}

/// Parse a binary property list.
pub(crate) fn parse(bytes: &[u8]) -> Result<Value, String> {
    if !bytes.starts_with(b"bplist0") || bytes.len() < 40 {
        return Err("not a binary property list".into());
    }
    let trailer = &bytes[bytes.len() - 32..];
    let offset_size = usize::from(trailer[6]);
    let ref_size = usize::from(trailer[7]);
    let word = |at: usize| usize::try_from(be(&trailer[at..at + 8])).unwrap_or(usize::MAX);
    let (count, top, table) = (word(8), word(16), word(24));
    if !(1..=8).contains(&offset_size) || !(1..=8).contains(&ref_size) {
        return Err("its trailer is damaged".into());
    }
    let end = count
        .checked_mul(offset_size)
        .and_then(|n| n.checked_add(table))
        .filter(|end| *end <= bytes.len())
        .ok_or("its offset table runs past the end")?;
    let offsets = bytes[table..end]
        .chunks_exact(offset_size)
        .map(|c| usize::try_from(be(c)).unwrap_or(usize::MAX))
        .collect();
    Reader {
        bytes,
        offsets,
        ref_size,
    }
    .object(top, 0)
}

impl Reader<'_> {
    fn slice(&self, at: usize, len: usize) -> Result<&[u8], String> {
        at.checked_add(len)
            .and_then(|end| self.bytes.get(at..end))
            .ok_or_else(|| "an object runs past the end".to_string())
    }

    /// The count a marker carries, and where its contents start.
    fn counted(&self, at: usize, low: u8) -> Result<(usize, usize), String> {
        if low != 0x0F {
            return Ok((usize::from(low), at + 1));
        }
        let marker = self.slice(at + 1, 1)?[0];
        if marker >> 4 != 1 {
            return Err("a count is not a number".into());
        }
        let len = 1usize << (marker & 0x0F).min(4);
        let n = be(self.slice(at + 2, len)?);
        Ok((usize::try_from(n).unwrap_or(usize::MAX), at + 2 + len))
    }

    fn refs(&self, at: usize, n: usize) -> Result<Vec<usize>, String> {
        let len = n.checked_mul(self.ref_size).ok_or("a count is too large")?;
        Ok(self
            .slice(at, len)?
            .chunks_exact(self.ref_size)
            .map(|c| usize::try_from(be(c)).unwrap_or(usize::MAX))
            .collect())
    }

    fn object(&self, id: usize, depth: usize) -> Result<Value, String> {
        if depth > DEEPEST {
            return Err("it nests too deep".into());
        }
        let at = *self.offsets.get(id).ok_or("a reference has no object")?;
        let marker = self.slice(at, 1)?[0];
        let (kind, low) = (marker >> 4, marker & 0x0F);
        Ok(match kind {
            0x0 => match low {
                8 => Value::Bool(false),
                9 => Value::Bool(true),
                _ => Value::Null,
            },
            0x1 => {
                let b = self.slice(at + 1, 1usize << low.min(4))?;
                // 1, 2 and 4 bytes are unsigned; 8 are signed, and 16 hold
                // an unsigned 8 in their low half
                #[allow(clippy::cast_possible_wrap)]
                Value::Int(be(&b[b.len().saturating_sub(8)..]) as i64)
            }
            0x2 | 0x3 => {
                let b = self.slice(at + 1, 1usize << low.min(3))?;
                #[allow(clippy::cast_possible_truncation)]
                let real = if b.len() == 4 {
                    f64::from(f32::from_bits(be(b) as u32))
                } else {
                    f64::from_bits(be(b))
                };
                if kind == 0x2 {
                    Value::Real(real)
                } else {
                    Value::Date(real)
                }
            }
            0x4 => {
                let (n, start) = self.counted(at, low)?;
                Value::Data(self.slice(start, n)?.to_vec())
            }
            0x5 => {
                let (n, start) = self.counted(at, low)?;
                Value::Str(String::from_utf8_lossy(self.slice(start, n)?).into_owned())
            }
            0x6 => {
                let (n, start) = self.counted(at, low)?;
                let len = n.checked_mul(2).ok_or("a count is too large")?;
                let units: Vec<u16> = self
                    .slice(start, len)?
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_be_bytes(*c))
                    .collect();
                Value::Str(String::from_utf16_lossy(&units))
            }
            0x8 => Value::Uid(
                usize::try_from(be(self.slice(at + 1, usize::from(low) + 1)?))
                    .unwrap_or(usize::MAX),
            ),
            0xA => {
                let (n, start) = self.counted(at, low)?;
                let items = self.refs(start, n)?;
                Value::Array(
                    items
                        .into_iter()
                        .map(|r| self.object(r, depth + 1))
                        .collect::<Result<_, _>>()?,
                )
            }
            0xD => {
                let (n, start) = self.counted(at, low)?;
                let refs = self.refs(start, n.checked_mul(2).ok_or("a count is too large")?)?;
                let (keys, values) = refs.split_at(n);
                let mut d = BTreeMap::new();
                for (k, v) in keys.iter().zip(values) {
                    if let Value::Str(k) = self.object(*k, depth + 1)? {
                        d.insert(k, self.object(*v, depth + 1)?);
                    }
                }
                Value::Dict(d)
            }
            other => return Err(format!("object type {other:#x} is not one this reads")),
        })
    }
}

/// A keyed archive's root object with its references followed: an archived
/// dictionary is a [`Value::Dict`], an array a [`Value::Array`], a date a
/// [`Value::Date`], and any other object a dictionary of its fields.
pub(crate) fn unarchive(plist: &Value) -> Result<Value, String> {
    let objects = plist
        .get("$objects")
        .and_then(Value::as_array)
        .ok_or("not a keyed archive")?;
    let top = plist.get("$top").ok_or("the archive has no root")?;
    let root = match top.get("root") {
        Some(r) => r,
        None => return Err("the archive has no root".into()),
    };
    follow(root, objects, 0)
}

fn follow(v: &Value, objects: &[Value], depth: usize) -> Result<Value, String> {
    if depth > DEEPEST {
        return Err("the archive nests too deep".into());
    }
    let go = |v: &Value| follow(v, objects, depth + 1);
    Ok(match v {
        Value::Uid(0) => Value::Null,
        Value::Uid(i) => go(objects.get(*i).ok_or("a reference has no object")?)?,
        Value::Dict(d) => match (d.get("NS.keys"), d.get("NS.objects")) {
            (Some(Value::Array(keys)), Some(Value::Array(values))) => {
                let mut out = BTreeMap::new();
                for (k, v) in keys.iter().zip(values) {
                    if let Value::Str(k) = go(k)? {
                        out.insert(k, go(v)?);
                    }
                }
                Value::Dict(out)
            }
            (None, Some(Value::Array(values))) => {
                Value::Array(values.iter().map(go).collect::<Result<_, _>>()?)
            }
            _ => match d.get("NS.time") {
                Some(Value::Real(t)) => Value::Date(*t),
                _ => Value::Dict(
                    d.iter()
                        .filter(|(k, _)| *k != "$class")
                        .map(|(k, v)| Ok((k.clone(), go(v)?)))
                        .collect::<Result<_, String>>()?,
                ),
            },
        },
        Value::Array(a) => Value::Array(a.iter().map(go).collect::<Result<_, _>>()?),
        other => other.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // `plutil -convert binary1` of {a: 1, s: "héllo", l: [true, 2.5], d: <0102>}
    const SMALL: &[u8] = include_bytes!("../tests/fixtures/small.bplist");

    #[test]
    fn every_kind_reads_as_itself() {
        let v = parse(SMALL).expect("parse");
        assert_eq!(v.get("a"), Some(&Value::Int(1)));
        assert_eq!(v.get("s").and_then(Value::as_str), Some("héllo"));
        assert_eq!(
            v.get("l").and_then(Value::as_array),
            Some(&[Value::Bool(true), Value::Real(2.5)][..])
        );
        assert_eq!(v.get("d"), Some(&Value::Data(vec![1, 2])));
    }

    #[test]
    fn a_damaged_file_is_an_error_not_a_panic() {
        assert!(parse(b"").is_err());
        assert!(parse(b"<?xml version=\"1.0\"?>").is_err());
        for cut in 8..SMALL.len() {
            let _ = parse(&SMALL[..cut]);
        }
        for i in 8..SMALL.len() {
            let mut bent = SMALL.to_vec();
            bent[i] ^= 0xFF;
            let _ = parse(&bent);
        }
    }
}
