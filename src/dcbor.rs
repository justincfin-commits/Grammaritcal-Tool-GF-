//! Deterministic CBOR, written by hand with no dependencies.
//!
//! The data type `Cbor` at the top of this module is plain data. The
//! functions below it encode a value into bytes, decode bytes back into a
//! value, and print a value in the diagnostic notation of RFC 8949 section 8.
//!
//! The encoder follows the deterministic rules of RFC 8949 section 4.2.1 and
//! the dCBOR profile: every integer head uses the shortest form that fits,
//! every length is definite, map keys are sorted by their encoded bytes, no
//! map holds one key twice, and no floating point value ever appears. The
//! decoder refuses any input that breaks one of those rules, so a round trip
//! through it is a check that the encoder obeyed them.
//!
//! This encoder is not the EverCBOR implementation and has not been checked
//! against it. The tests in `tests/dcbor.rs` hold the vectors from RFC 8949
//! appendix A, and the command line program can write the encoding of every
//! result to disk so that a later check against another implementation stays
//! possible.

use std::fmt::Write;

/// One CBOR data item. The tool never builds a floating point item.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Cbor {
    /// A non-negative whole number.
    Uint(u64),
    /// A negative whole number `-1 - n`, stored as `n`.
    Nint(u64),
    /// A byte string.
    Bytes(Vec<u8>),
    /// A text string.
    Text(String),
    /// A list of items.
    Array(Vec<Cbor>),
    /// A map from items to items. The encoder sorts the pairs itself.
    Map(Vec<(Cbor, Cbor)>),
    /// A boolean.
    Bool(bool),
    /// The null value.
    Null,
}

/// A failure while encoding or decoding. Every `at` is a byte offset.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum CborError {
    /// A map holds one key twice.
    DuplicateKey,
    /// The bytes ended before the item did.
    Truncated { at: usize },
    /// The bytes hold a head the decoder does not accept.
    UnsupportedHead { at: usize, byte: u8 },
    /// An integer head is longer than the shortest form that fits its value.
    NotShortest { at: usize },
    /// A map's keys are not in ascending order of their encoded bytes.
    KeysNotSorted { at: usize },
    /// A text string is not valid UTF-8.
    BadText { at: usize },
    /// The decoder reached its nesting limit.
    TooDeep { at: usize },
    /// The bytes hold more than one item.
    TrailingBytes { at: usize },
}

/// Appends the head of an item: the major type and the argument, shortest form.
fn put_head(out: &mut Vec<u8>, major: u8, argument: u64) {
    let high = major << 5;
    if argument < 24 {
        out.push(high | (argument as u8));
    } else if argument <= u64::from(u8::MAX) {
        out.push(high | 24);
        out.push(argument as u8);
    } else if argument <= u64::from(u16::MAX) {
        out.push(high | 25);
        out.extend_from_slice(&(argument as u16).to_be_bytes());
    } else if argument <= u64::from(u32::MAX) {
        out.push(high | 26);
        out.extend_from_slice(&(argument as u32).to_be_bytes());
    } else {
        out.push(high | 27);
        out.extend_from_slice(&argument.to_be_bytes());
    }
}

/// Encodes one item into fresh bytes.
pub fn encode(value: &Cbor) -> Result<Vec<u8>, CborError> {
    let mut out: Vec<u8> = Vec::new();
    encode_into(value, &mut out)?;
    Ok(out)
}

fn encode_into(value: &Cbor, out: &mut Vec<u8>) -> Result<(), CborError> {
    match value {
        Cbor::Uint(n) => put_head(out, 0, *n),
        Cbor::Nint(n) => put_head(out, 1, *n),
        Cbor::Bytes(bytes) => {
            put_head(out, 2, bytes.len() as u64);
            out.extend_from_slice(bytes);
        }
        Cbor::Text(text) => {
            put_head(out, 3, text.len() as u64);
            out.extend_from_slice(text.as_bytes());
        }
        Cbor::Array(items) => {
            put_head(out, 4, items.len() as u64);
            for item in items {
                encode_into(item, out)?;
            }
        }
        Cbor::Map(pairs) => {
            let mut encoded: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
            for (key, item) in pairs {
                encoded.push((encode(key)?, encode(item)?));
            }
            encoded.sort_by(|left, right| left.0.cmp(&right.0));
            for window in encoded.windows(2) {
                if let [left, right] = window {
                    if left.0 == right.0 {
                        return Err(CborError::DuplicateKey);
                    }
                }
            }
            put_head(out, 5, encoded.len() as u64);
            for (key, item) in encoded {
                out.extend_from_slice(&key);
                out.extend_from_slice(&item);
            }
        }
        Cbor::Bool(false) => out.push(0xf4),
        Cbor::Bool(true) => out.push(0xf5),
        Cbor::Null => out.push(0xf6),
    }
    Ok(())
}

/// How deep the decoder follows nested arrays and maps.
const MAX_DEPTH: usize = 64;

/// Reads `count` bytes at `at`.
fn take(bytes: &[u8], at: usize, count: usize) -> Result<&[u8], CborError> {
    let end = at.checked_add(count).ok_or(CborError::Truncated { at })?;
    bytes.get(at..end).ok_or(CborError::Truncated { at })
}

/// Reads one head and returns the major type, the argument, and the next
/// position. Refuses a head that is longer than its value needs.
fn read_head(bytes: &[u8], at: usize) -> Result<(u8, u64, usize), CborError> {
    let first = *bytes.get(at).ok_or(CborError::Truncated { at })?;
    let major = first >> 5;
    let low = first & 0x1f;
    let next = at.saturating_add(1);
    match low {
        0..=23 => Ok((major, u64::from(low), next)),
        24 => {
            let value = u64::from(*bytes.get(next).ok_or(CborError::Truncated { at: next })?);
            if value < 24 {
                return Err(CborError::NotShortest { at });
            }
            Ok((major, value, next.saturating_add(1)))
        }
        25 => {
            let slice = take(bytes, next, 2)?;
            let mut buffer = [0u8; 2];
            buffer.copy_from_slice(slice);
            let value = u64::from(u16::from_be_bytes(buffer));
            if value <= u64::from(u8::MAX) {
                return Err(CborError::NotShortest { at });
            }
            Ok((major, value, next.saturating_add(2)))
        }
        26 => {
            let slice = take(bytes, next, 4)?;
            let mut buffer = [0u8; 4];
            buffer.copy_from_slice(slice);
            let value = u64::from(u32::from_be_bytes(buffer));
            if value <= u64::from(u16::MAX) {
                return Err(CborError::NotShortest { at });
            }
            Ok((major, value, next.saturating_add(4)))
        }
        27 => {
            let slice = take(bytes, next, 8)?;
            let mut buffer = [0u8; 8];
            buffer.copy_from_slice(slice);
            let value = u64::from_be_bytes(buffer);
            if value <= u64::from(u32::MAX) {
                return Err(CborError::NotShortest { at });
            }
            Ok((major, value, next.saturating_add(8)))
        }
        _ => Err(CborError::UnsupportedHead { at, byte: first }),
    }
}

/// Decodes one item from the whole byte string.
pub fn decode(bytes: &[u8]) -> Result<Cbor, CborError> {
    let (value, next) = decode_at(bytes, 0, MAX_DEPTH)?;
    if next != bytes.len() {
        return Err(CborError::TrailingBytes { at: next });
    }
    Ok(value)
}

fn decode_at(bytes: &[u8], at: usize, depth: usize) -> Result<(Cbor, usize), CborError> {
    let deeper = depth.checked_sub(1).ok_or(CborError::TooDeep { at })?;
    let first = *bytes.get(at).ok_or(CborError::Truncated { at })?;
    match first {
        0xf4 => return Ok((Cbor::Bool(false), at.saturating_add(1))),
        0xf5 => return Ok((Cbor::Bool(true), at.saturating_add(1))),
        0xf6 => return Ok((Cbor::Null, at.saturating_add(1))),
        // Every other simple value, every floating point number, and every
        // tag is outside the profile this tool writes.
        _ if first >> 5 >= 6 => return Err(CborError::UnsupportedHead { at, byte: first }),
        _ => {}
    }
    let (major, argument, next) = read_head(bytes, at)?;
    let count = usize::try_from(argument).map_err(|_| CborError::Truncated { at })?;
    match major {
        0 => Ok((Cbor::Uint(argument), next)),
        1 => Ok((Cbor::Nint(argument), next)),
        2 => {
            let slice = take(bytes, next, count)?;
            Ok((Cbor::Bytes(slice.to_vec()), next.saturating_add(count)))
        }
        3 => {
            let slice = take(bytes, next, count)?;
            let text = String::from_utf8(slice.to_vec()).map_err(|_| CborError::BadText { at })?;
            Ok((Cbor::Text(text), next.saturating_add(count)))
        }
        4 => {
            let mut items: Vec<Cbor> = Vec::new();
            let mut here = next;
            for _ in 0..count {
                let (item, after) = decode_at(bytes, here, deeper)?;
                items.push(item);
                here = after;
            }
            Ok((Cbor::Array(items), here))
        }
        5 => {
            let mut pairs: Vec<(Cbor, Cbor)> = Vec::new();
            let mut here = next;
            let mut previous: Option<&[u8]> = None;
            for _ in 0..count {
                let key_start = here;
                let (key, after_key) = decode_at(bytes, here, deeper)?;
                let key_bytes = bytes
                    .get(key_start..after_key)
                    .ok_or(CborError::Truncated { at: key_start })?;
                if let Some(earlier) = previous {
                    if earlier >= key_bytes {
                        return Err(CborError::KeysNotSorted { at: key_start });
                    }
                }
                previous = Some(key_bytes);
                let (item, after_item) = decode_at(bytes, after_key, deeper)?;
                pairs.push((key, item));
                here = after_item;
            }
            Ok((Cbor::Map(pairs), here))
        }
        _ => Err(CborError::UnsupportedHead { at, byte: first }),
    }
}

/// Prints one item in the diagnostic notation of RFC 8949 section 8.
pub fn diagnostic(value: &Cbor) -> String {
    let mut out = String::new();
    write_diagnostic(value, &mut out);
    out
}

fn write_diagnostic(value: &Cbor, out: &mut String) {
    match value {
        Cbor::Uint(n) => {
            let _ = write!(out, "{n}");
        }
        Cbor::Nint(n) => {
            // -1 - n, printed without overflow for every u64.
            let _ = write!(out, "-{}", u128::from(*n).saturating_add(1));
        }
        Cbor::Bytes(bytes) => {
            out.push_str("h'");
            for byte in bytes {
                let _ = write!(out, "{byte:02x}");
            }
            out.push('\'');
        }
        Cbor::Text(text) => {
            out.push('"');
            for c in text.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    other if (other as u32) < 0x20 => {
                        let _ = write!(out, "\\u{:04x}", other as u32);
                    }
                    other => out.push(other),
                }
            }
            out.push('"');
        }
        Cbor::Array(items) => {
            out.push('[');
            for (position, item) in items.iter().enumerate() {
                if position > 0 {
                    out.push_str(", ");
                }
                write_diagnostic(item, out);
            }
            out.push(']');
        }
        Cbor::Map(pairs) => {
            // Print in encoded key order, the same order the bytes use.
            let mut sorted: Vec<(Vec<u8>, &Cbor, &Cbor)> = pairs
                .iter()
                .map(|(key, item)| (encode(key).unwrap_or_default(), key, item))
                .collect();
            sorted.sort_by(|left, right| left.0.cmp(&right.0));
            out.push('{');
            for (position, (_, key, item)) in sorted.iter().enumerate() {
                if position > 0 {
                    out.push_str(", ");
                }
                write_diagnostic(key, out);
                out.push_str(": ");
                write_diagnostic(item, out);
            }
            out.push('}');
        }
        Cbor::Bool(true) => out.push_str("true"),
        Cbor::Bool(false) => out.push_str("false"),
        Cbor::Null => out.push_str("null"),
    }
}
