//! Model of `put_head` and `read_head` in `src/dcbor.rs`.
//!
//! A CBOR head is one byte holding the major type in its top three bits and
//! a five bit argument, followed by zero, one, two, four, or eight bytes of
//! a big-endian value when the argument is 24, 25, 26, or 27. The
//! deterministic rules of RFC 8949 section 4.2.1 demand the shortest form
//! that fits the value. The reader here refuses any longer form, and the
//! round trip proved below shows that the writer always produces the form
//! the reader accepts, so the writer always produces the shortest form.
//!
//! The model uses multiplication and division by constants instead of
//! shifts and masks, so the arithmetic stays linear for the solver. The two
//! are the same function on these ranges.

use vstd::prelude::*;

verus! {

/// The number of bytes the shortest head for `value` takes.
pub open spec fn head_len(value: u64) -> nat {
    if value < 24 {
        1
    } else if value <= 0xff {
        2
    } else if value <= 0xffff {
        3
    } else if value <= 0xffff_ffff {
        5
    } else {
        9
    }
}

/// `256 ^ n`.
pub open spec fn pow256(n: nat) -> nat
    decreases n,
{
    if n == 0 {
        1
    } else {
        256 * pow256((n - 1) as nat)
    }
}

proof fn lemma_pow256_values()
    ensures
        pow256(0) == 1,
        pow256(1) == 0x100,
        pow256(2) == 0x1_0000,
        pow256(4) == 0x1_0000_0000,
        pow256(8) == 0x1_0000_0000_0000_0000,
{
    reveal_with_fuel(pow256, 10);
}

/// The value that a big-endian byte list denotes.
pub open spec fn be_value(bytes: Seq<u8>) -> int
    decreases bytes.len(),
{
    if bytes.len() == 0 {
        0
    } else {
        be_value(bytes.drop_last()) * 256 + bytes.last()
    }
}

/// Appends the `width` big-endian bytes of `value`, most significant first.
///
/// The recursion writes the high bytes first, then the lowest byte, which is
/// exactly how `be_value` takes a list apart, so each level of the proof is
/// the one identity `value == (value / 256) * 256 + value % 256`.
fn be_bytes(value: u64, width: usize, out: &mut Vec<u8>)
    requires
        width <= 8,
        value < pow256(width as nat),
    ensures
        final(out).len() == old(out).len() + width,
        forall|i: int| 0 <= i < old(out).len() ==> final(out)@[i] == old(out)@[i],
        be_value(final(out)@.subrange(old(out).len() as int, final(out).len() as int)) == value,
    decreases width,
{
    if width == 0 {
        assert(out@.subrange(old(out).len() as int, out.len() as int) =~= Seq::<u8>::empty());
        return;
    }
    let ghost start = out.len() as int;
    proof {
        reveal_with_fuel(pow256, 10);
    }
    be_bytes(value / 256, width - 1, out);
    let ghost before_push = out@;
    out.push((value % 256) as u8);
    proof {
        assert(out@.subrange(start, out.len() as int).drop_last()
            =~= before_push.subrange(start, before_push.len() as int));
        assert(out@.subrange(start, out.len() as int).last() == (value % 256) as u8);
    }
}

/// Writes the shortest head for `value` with major type `major`.
pub fn put_head(major: u8, value: u64) -> (out: Vec<u8>)
    requires
        major < 8,
    ensures
        out.len() == head_len(value),
        out@[0] / 32 == major,
        value < 24 ==> out@[0] % 32 == value,
        24 <= value <= 0xff ==> out@[0] % 32 == 24 && be_value(out@.subrange(1, 2)) == value,
        0xff < value <= 0xffff ==> out@[0] % 32 == 25 && be_value(out@.subrange(1, 3)) == value,
        0xffff < value <= 0xffff_ffff ==> out@[0] % 32 == 26 && be_value(out@.subrange(1, 5)) == value,
        0xffff_ffff < value ==> out@[0] % 32 == 27 && be_value(out@.subrange(1, 9)) == value,
{
    proof {
        lemma_pow256_values();
    }
    let high: u8 = major * 32;
    let mut out: Vec<u8> = Vec::new();
    if value < 24 {
        out.push(high + (value as u8));
    } else if value <= 0xff {
        out.push(high + 24);
        be_bytes(value, 1, &mut out);
    } else if value <= 0xffff {
        out.push(high + 25);
        be_bytes(value, 2, &mut out);
    } else if value <= 0xffff_ffff {
        out.push(high + 26);
        be_bytes(value, 4, &mut out);
    } else {
        out.push(high + 27);
        be_bytes(value, 8, &mut out);
    }
    out
}

proof fn lemma_pow256_step(n: nat)
    requires
        n < 8,
    ensures
        pow256(n + 1) == 256 * pow256(n),
        pow256(n + 1) <= 0x1_0000_0000_0000_0000,
{
    reveal_with_fuel(pow256, 10);
}

/// Reads the big-endian value of `width` bytes starting at `at`.
fn read_be(bytes: &Vec<u8>, at: usize, width: usize) -> (value: u64)
    requires
        width <= 8,
        at + width <= bytes.len(),
    ensures
        value == be_value(bytes@.subrange(at as int, (at + width) as int)),
        value < pow256(width as nat),
{
    let mut value: u64 = 0;
    let mut i: usize = 0;
    proof {
        reveal_with_fuel(pow256, 10);
        assert(bytes@.subrange(at as int, at as int) =~= Seq::<u8>::empty());
    }
    while i < width
        invariant
            i <= width,
            width <= 8,
            at + width <= bytes.len(),
            value == be_value(bytes@.subrange(at as int, (at + i) as int)),
            value < pow256(i as nat),
        decreases width - i,
    {
        proof {
            lemma_pow256_step(i as nat);
            assert(bytes@.subrange(at as int, (at + i + 1) as int).drop_last()
                =~= bytes@.subrange(at as int, (at + i) as int));
            assert(bytes@.subrange(at as int, (at + i + 1) as int).last() == bytes@[(at + i) as int]);
        }
        value = value * 256 + (bytes[at + i] as u64);
        i = i + 1;
    }
    value
}

/// Reads one head. Returns the major type, the value, and the number of
/// bytes read, or nothing when the head is malformed or not the shortest.
pub fn read_head(bytes: &Vec<u8>) -> (found: Option<(u8, u64, usize)>)
    ensures
        found.is_some() ==> found.unwrap().2 <= bytes.len(),
        found.is_some() ==> found.unwrap().2 == head_len(found.unwrap().1),
        bytes.len() >= 1 && bytes@[0] % 32 < 24
            ==> found == Some((bytes@[0] / 32, (bytes@[0] % 32) as u64, 1usize)),
        bytes.len() >= 2 && bytes@[0] % 32 == 24 && be_value(bytes@.subrange(1, 2)) >= 24
            ==> found == Some((bytes@[0] / 32, be_value(bytes@.subrange(1, 2)) as u64, 2usize)),
        bytes.len() >= 3 && bytes@[0] % 32 == 25 && be_value(bytes@.subrange(1, 3)) > 0xff
            ==> found == Some((bytes@[0] / 32, be_value(bytes@.subrange(1, 3)) as u64, 3usize)),
        bytes.len() >= 5 && bytes@[0] % 32 == 26 && be_value(bytes@.subrange(1, 5)) > 0xffff
            ==> found == Some((bytes@[0] / 32, be_value(bytes@.subrange(1, 5)) as u64, 5usize)),
        bytes.len() >= 9 && bytes@[0] % 32 == 27 && be_value(bytes@.subrange(1, 9)) > 0xffff_ffff
            ==> found == Some((bytes@[0] / 32, be_value(bytes@.subrange(1, 9)) as u64, 9usize)),
{
    proof {
        lemma_pow256_values();
    }
    if bytes.len() == 0 {
        return None;
    }
    let first = bytes[0];
    let major = first / 32;
    let low = first % 32;
    if low < 24 {
        Some((major, low as u64, 1))
    } else if low == 24 {
        if bytes.len() < 2 {
            return None;
        }
        let value = read_be(bytes, 1, 1);
        if value < 24 {
            return None;
        }
        Some((major, value, 2))
    } else if low == 25 {
        if bytes.len() < 3 {
            return None;
        }
        let value = read_be(bytes, 1, 2);
        if value <= 0xff {
            return None;
        }
        Some((major, value, 3))
    } else if low == 26 {
        if bytes.len() < 5 {
            return None;
        }
        let value = read_be(bytes, 1, 4);
        if value <= 0xffff {
            return None;
        }
        Some((major, value, 5))
    } else if low == 27 {
        if bytes.len() < 9 {
            return None;
        }
        let value = read_be(bytes, 1, 8);
        if value <= 0xffff_ffff {
            return None;
        }
        Some((major, value, 9))
    } else {
        None
    }
}

/// Reading back what `put_head` wrote gives the major type, the value, and
/// the whole length. So the writer produces exactly the form the reader
/// accepts, and the reader accepts only the shortest form.
pub fn round_trip(major: u8, value: u64) -> (found: Option<(u8, u64, usize)>)
    requires
        major < 8,
    ensures
        found == Some((major, value, head_len(value) as usize)),
{
    let out = put_head(major, value);
    read_head(&out)
}

} // verus!
