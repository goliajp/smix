//! The smallest XML plist that can carry the usbmux protocol.
//!
//! usbmux speaks plist, but it speaks a tiny corner of it: dictionaries
//! of strings and integers, and one array of dictionaries in the device
//! listing. That is the whole vocabulary.
//!
//! So this is that corner and nothing else. Writing a general plist
//! library here would be building a second thing to maintain in order to
//! use four types — and pulling one in would be a dependency for a format
//! whose used subset fits on a page (IR-1: what cannot be self-made is
//! the daemon, and Apple already ships that).
//!
//! What it will not do, it says so: an unsupported type is an error, not
//! a silently dropped key. A parser that skips what it does not
//! understand turns a protocol change into missing data rather than into
//! a message.

use std::collections::BTreeMap;

/// A plist value, in the subset usbmux uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// `<string>`
    Str(String),
    /// `<integer>`
    Int(i64),
    /// `<dict>`
    Dict(BTreeMap<String, Value>),
    /// `<array>`
    Array(Vec<Value>),
    /// `<true/>` / `<false/>`
    Bool(bool),
    /// `<data>`, kept as its base64 text. usbmux does not use it in the
    /// messages this crate sends, but device listings can carry it, and
    /// dropping a key silently is what this module refuses to do.
    Data(String),
}

impl Value {
    /// Borrow a dictionary entry.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Dict(d) => d.get(key),
            _ => None,
        }
    }

    /// This value as a string, if it is one.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// This value as an integer, if it is one.
    #[must_use]
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// This value as an array, if it is one.
    #[must_use]
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }
}

/// Why a plist could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PlistError {
    /// The bytes are not valid UTF-8.
    #[error("plist is not UTF-8: {0}")]
    NotUtf8(String),
    /// The document ended in the middle of something.
    #[error("plist ended unexpectedly (in {context})")]
    Truncated {
        /// What was being read when it ran out.
        context: String,
    },
    /// A tag this subset does not handle.
    ///
    /// Reported rather than skipped: a parser that ignores what it does
    /// not understand turns a protocol change into missing data instead
    /// of into a message.
    #[error("plist contains <{tag}>, which this subset does not handle")]
    Unsupported {
        /// The tag name.
        tag: String,
    },
    /// Structurally wrong — a close without an open, a key outside a dict.
    #[error("malformed plist: {detail}")]
    Malformed {
        /// What was wrong.
        detail: String,
    },
}

/// Render a value as an XML plist document.
#[must_use]
pub fn to_xml(value: &Value) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n",
    );
    write_value(value, &mut out);
    out.push_str("</plist>\n");
    out
}

fn write_value(v: &Value, out: &mut String) {
    match v {
        Value::Str(s) => {
            out.push_str("<string>");
            escape_into(s, out);
            out.push_str("</string>\n");
        }
        Value::Int(i) => {
            out.push_str(&format!("<integer>{i}</integer>\n"));
        }
        Value::Bool(b) => out.push_str(if *b { "<true/>\n" } else { "<false/>\n" }),
        Value::Data(d) => {
            out.push_str("<data>");
            escape_into(d, out);
            out.push_str("</data>\n");
        }
        Value::Dict(d) => {
            out.push_str("<dict>\n");
            for (k, val) in d {
                out.push_str("<key>");
                escape_into(k, out);
                out.push_str("</key>\n");
                write_value(val, out);
            }
            out.push_str("</dict>\n");
        }
        Value::Array(a) => {
            out.push_str("<array>\n");
            for val in a {
                write_value(val, out);
            }
            out.push_str("</array>\n");
        }
    }
}

fn escape_into(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

/// Parse an XML plist document.
///
/// # Errors
///
/// Returns [`PlistError`] for non-UTF-8 input, a truncated document, a
/// tag outside this subset, or structural nonsense.
pub fn from_xml(bytes: &[u8]) -> Result<Value, PlistError> {
    let text = std::str::from_utf8(bytes).map_err(|e| PlistError::NotUtf8(e.to_string()))?;
    let mut p = Parser { s: text, i: 0 };
    p.skip_prologue();
    let v = p.parse_value()?;
    Ok(v)
}

struct Parser<'a> {
    s: &'a str,
    i: usize,
}

impl<'a> Parser<'a> {
    fn skip_prologue(&mut self) {
        // Everything up to and including `<plist ...>`; if there is no
        // plist element the value parser will report what it found.
        if let Some(pos) = self.s.find("<plist")
            && let Some(end) = self.s[pos..].find('>')
        {
            self.i = pos + end + 1;
        }
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.s[self.i..].chars().next() {
            if c.is_whitespace() {
                self.i += c.len_utf8();
            } else {
                break;
            }
        }
    }

    /// Read the next tag, e.g. `dict`, `/dict`, `true/`.
    fn next_tag(&mut self) -> Result<String, PlistError> {
        self.skip_ws();
        if !self.s[self.i..].starts_with('<') {
            return Err(PlistError::Truncated {
                context: "expected a tag".into(),
            });
        }
        let rest = &self.s[self.i + 1..];
        let end = rest.find('>').ok_or_else(|| PlistError::Truncated {
            context: "unclosed tag".into(),
        })?;
        let tag = rest[..end].to_string();
        self.i += 1 + end + 1;
        Ok(tag)
    }

    fn text_until_close(&mut self, tag: &str) -> Result<String, PlistError> {
        let close = format!("</{tag}>");
        let rest = &self.s[self.i..];
        let end = rest.find(&close).ok_or_else(|| PlistError::Truncated {
            context: format!("<{tag}> without its close"),
        })?;
        let raw = &rest[..end];
        self.i += end + close.len();
        Ok(unescape(raw))
    }

    fn parse_value(&mut self) -> Result<Value, PlistError> {
        let tag = self.next_tag()?;
        self.value_for_tag(&tag)
    }

    fn value_for_tag(&mut self, tag: &str) -> Result<Value, PlistError> {
        match tag {
            "string" => Ok(Value::Str(self.text_until_close("string")?)),
            "integer" => {
                let t = self.text_until_close("integer")?;
                t.trim()
                    .parse::<i64>()
                    .map(Value::Int)
                    .map_err(|_| PlistError::Malformed {
                        detail: format!("<integer> holds {t:?}"),
                    })
            }
            "data" => Ok(Value::Data(self.text_until_close("data")?)),
            "true/" => Ok(Value::Bool(true)),
            "false/" => Ok(Value::Bool(false)),
            // Self-closing empty containers.
            //
            // Learned from the wire, not from a round trip: `ListDevices`
            // with nothing attached answers `<array/>`, and a round-trip
            // test can never produce that form because the encoder always
            // writes the long one. The empty case is exactly the case a
            // caller most needs read correctly — it is the difference
            // between "no devices" and an error.
            "array/" => Ok(Value::Array(Vec::new())),
            "dict/" => Ok(Value::Dict(BTreeMap::new())),
            "dict" => {
                let mut map = BTreeMap::new();
                loop {
                    let t = self.next_tag()?;
                    if t == "/dict" {
                        return Ok(Value::Dict(map));
                    }
                    if t != "key" {
                        return Err(PlistError::Malformed {
                            detail: format!("expected <key> in <dict>, found <{t}>"),
                        });
                    }
                    let key = self.text_until_close("key")?;
                    let val = self.parse_value()?;
                    map.insert(key, val);
                }
            }
            "array" => {
                let mut items = Vec::new();
                loop {
                    let t = self.next_tag()?;
                    if t == "/array" {
                        return Ok(Value::Array(items));
                    }
                    items.push(self.value_for_tag(&t)?);
                }
            }
            other => Err(PlistError::Unsupported {
                tag: other.to_string(),
            }),
        }
    }
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(pairs: &[(&str, Value)]) -> Value {
        Value::Dict(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), v.clone()))
                .collect(),
        )
    }

    #[test]
    fn a_request_roundtrips() {
        let v = dict(&[
            ("MessageType", Value::Str("Connect".into())),
            ("DeviceID", Value::Int(3)),
            ("PortNumber", Value::Int(39222)),
        ]);
        let xml = to_xml(&v);
        let back = from_xml(xml.as_bytes()).expect("parse");
        assert_eq!(v, back);
    }

    #[test]
    fn a_device_listing_shape_parses() {
        // The nesting usbmux actually sends: a dict holding an array of
        // dicts, each with a nested Properties dict.
        let v = dict(&[(
            "DeviceList",
            Value::Array(vec![dict(&[
                ("DeviceID", Value::Int(3)),
                (
                    "Properties",
                    dict(&[
                        ("ConnectionType", Value::Str("USB".into())),
                        (
                            "SerialNumber",
                            Value::Str("00008120-001410C11A42201E".into()),
                        ),
                    ]),
                ),
            ])]),
        )]);
        let back = from_xml(to_xml(&v).as_bytes()).expect("parse");
        let dev = &back.get("DeviceList").unwrap().as_array().unwrap()[0];
        assert_eq!(dev.get("DeviceID").unwrap().as_int(), Some(3));
        assert_eq!(
            dev.get("Properties")
                .unwrap()
                .get("SerialNumber")
                .unwrap()
                .as_str(),
            Some("00008120-001410C11A42201E")
        );
    }

    #[test]
    fn a_truncated_document_is_an_error_not_an_empty_dict() {
        // An empty result would make "no devices" and "read badly" the
        // same answer.
        let xml = "<plist version=\"1.0\"><dict><key>DeviceList</key><array>";
        match from_xml(xml.as_bytes()) {
            Err(PlistError::Truncated { .. }) => {}
            other => panic!("expected Truncated, got {other:?}"),
        }
    }

    #[test]
    fn a_tag_outside_the_subset_is_reported_not_skipped() {
        // Skipping would turn a protocol change into missing data.
        let xml =
            "<plist version=\"1.0\"><dict><key>When</key><date>2026-08-06</date></dict></plist>";
        match from_xml(xml.as_bytes()) {
            Err(PlistError::Unsupported { tag }) => assert_eq!(tag, "date"),
            other => panic!("expected Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn a_self_closing_empty_array_is_an_empty_array() {
        // From the wire: `ListDevices` with nothing plugged in answers
        // `<array/>`. The round-trip tests above cannot produce this —
        // the encoder always writes `<array>…</array>` — so this form
        // arrived only when a real daemon sent it.
        let xml = "<plist version=\"1.0\"><dict><key>DeviceList</key><array/></dict></plist>";
        let v = from_xml(xml.as_bytes()).expect("parse");
        assert_eq!(
            v.get("DeviceList").and_then(Value::as_array),
            Some([].as_slice())
        );
    }

    #[test]
    fn a_self_closing_empty_dict_is_an_empty_dict() {
        let xml = "<plist version=\"1.0\"><dict><key>Properties</key><dict/></dict></plist>";
        let v = from_xml(xml.as_bytes()).expect("parse");
        assert_eq!(v.get("Properties"), Some(&Value::Dict(BTreeMap::new())));
    }

    #[test]
    fn markup_in_a_string_survives_the_round_trip() {
        let v = dict(&[("ProgName", Value::Str("smix <2> & co".into()))]);
        let back = from_xml(to_xml(&v).as_bytes()).expect("parse");
        assert_eq!(
            back.get("ProgName").unwrap().as_str(),
            Some("smix <2> & co")
        );
    }
}
