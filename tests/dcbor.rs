//! Tests for the deterministic CBOR encoder and decoder.

use gf_mltt::dcbor::{decode, diagnostic, encode, Cbor, CborError};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn text(s: &str) -> Cbor {
    Cbor::Text(s.to_string())
}

#[test]
fn the_vectors_from_rfc_8949_appendix_a_encode_as_published() {
    let cases: Vec<(Cbor, &str)> = vec![
        (Cbor::Uint(0), "00"),
        (Cbor::Uint(1), "01"),
        (Cbor::Uint(10), "0a"),
        (Cbor::Uint(23), "17"),
        (Cbor::Uint(24), "1818"),
        (Cbor::Uint(25), "1819"),
        (Cbor::Uint(100), "1864"),
        (Cbor::Uint(1000), "1903e8"),
        (Cbor::Uint(1_000_000), "1a000f4240"),
        (Cbor::Uint(1_000_000_000_000), "1b000000e8d4a51000"),
        (Cbor::Uint(u64::MAX), "1bffffffffffffffff"),
        (Cbor::Nint(0), "20"),
        (Cbor::Nint(9), "29"),
        (Cbor::Nint(99), "3863"),
        (Cbor::Nint(999), "3903e7"),
        (Cbor::Bool(false), "f4"),
        (Cbor::Bool(true), "f5"),
        (Cbor::Null, "f6"),
        (Cbor::Bytes(vec![]), "40"),
        (Cbor::Bytes(vec![1, 2, 3, 4]), "4401020304"),
        (text(""), "60"),
        (text("a"), "6161"),
        (text("IETF"), "6449455446"),
        (text("\"\\"), "62225c"),
        (text("\u{fc}"), "62c3bc"),
        (text("\u{6c34}"), "63e6b0b4"),
        (Cbor::Array(vec![]), "80"),
        (Cbor::Array(vec![Cbor::Uint(1), Cbor::Uint(2), Cbor::Uint(3)]), "83010203"),
        (
            Cbor::Array(vec![
                Cbor::Uint(1),
                Cbor::Array(vec![Cbor::Uint(2), Cbor::Uint(3)]),
                Cbor::Array(vec![Cbor::Uint(4), Cbor::Uint(5)]),
            ]),
            "8301820203820405",
        ),
        (Cbor::Map(vec![]), "a0"),
        (
            Cbor::Map(vec![(Cbor::Uint(1), Cbor::Uint(2)), (Cbor::Uint(3), Cbor::Uint(4))]),
            "a201020304",
        ),
        (
            Cbor::Map(vec![
                (text("a"), Cbor::Uint(1)),
                (text("b"), Cbor::Array(vec![Cbor::Uint(2), Cbor::Uint(3)])),
            ]),
            "a26161016162820203",
        ),
        (
            Cbor::Array(vec![text("a"), Cbor::Map(vec![(text("b"), text("c"))])]),
            "826161a161626163",
        ),
    ];
    for (value, expected) in cases {
        assert_eq!(hex(&encode(&value).unwrap()), expected, "{value:?}");
    }
}

#[test]
fn map_keys_are_sorted_by_their_encoded_bytes_whatever_order_they_arrive_in() {
    let forward = Cbor::Map(vec![
        (text("a"), Cbor::Uint(1)),
        (text("b"), Cbor::Uint(2)),
        (Cbor::Uint(10), Cbor::Uint(3)),
        (Cbor::Uint(100), Cbor::Uint(4)),
        (text("aa"), Cbor::Uint(5)),
    ]);
    let backward = Cbor::Map(vec![
        (text("aa"), Cbor::Uint(5)),
        (Cbor::Uint(100), Cbor::Uint(4)),
        (Cbor::Uint(10), Cbor::Uint(3)),
        (text("b"), Cbor::Uint(2)),
        (text("a"), Cbor::Uint(1)),
    ]);
    let bytes = encode(&forward).unwrap();
    assert_eq!(bytes, encode(&backward).unwrap());
    // Keys sort by the bytes of their encoding, compared byte by byte.
    assert_eq!(hex(&bytes), "a50a0318640461610161620262616105");
    // RFC 8949 sorts byte by byte, not shorter first: 0x19 sorts before 0x61
    // even though the two byte text "z" is shorter than the three byte 300.
    let mixed = Cbor::Map(vec![(text("z"), Cbor::Uint(1)), (Cbor::Uint(300), Cbor::Uint(2))]);
    assert_eq!(hex(&encode(&mixed).unwrap()), "a219012c02617a01");
}

#[test]
fn a_duplicate_key_is_refused() {
    let value = Cbor::Map(vec![(text("a"), Cbor::Uint(1)), (text("a"), Cbor::Uint(2))]);
    assert_eq!(encode(&value).unwrap_err(), CborError::DuplicateKey);
}

#[test]
fn every_encoding_decodes_back_to_the_same_value() {
    let values = vec![
        Cbor::Uint(0),
        Cbor::Uint(24),
        Cbor::Uint(256),
        Cbor::Uint(65_536),
        Cbor::Uint(u64::MAX),
        Cbor::Nint(0),
        Cbor::Nint(1_000_000),
        Cbor::Bytes(vec![0, 255]),
        text("hello \"world\"\n"),
        Cbor::Array(vec![Cbor::Null, Cbor::Bool(true), text("x")]),
        Cbor::Map(vec![
            (text("z"), Cbor::Uint(1)),
            (text("a"), Cbor::Array(vec![Cbor::Map(vec![])])),
        ]),
    ];
    for value in values {
        let bytes = encode(&value).unwrap();
        let back = decode(&bytes).unwrap();
        // A map decodes in sorted order, so compare through a second encoding.
        assert_eq!(encode(&back).unwrap(), bytes, "{value:?}");
    }
}

#[test]
fn the_decoder_refuses_a_head_that_is_not_shortest_and_an_unsorted_map() {
    // 0x18 0x05 says "one byte follows" for the value 5, which fits in the head.
    assert_eq!(decode(&[0x18, 0x05]).unwrap_err(), CborError::NotShortest { at: 0 });
    // 0x19 0x00 0xff says "two bytes follow" for 255, which fits in one.
    assert_eq!(decode(&[0x19, 0x00, 0xff]).unwrap_err(), CborError::NotShortest { at: 0 });
    // { "b": 1, "a": 2 } with the keys in the wrong order.
    let bad = [0xa2, 0x61, 0x62, 0x01, 0x61, 0x61, 0x02];
    assert_eq!(decode(&bad).unwrap_err(), CborError::KeysNotSorted { at: 4 });
    // Trailing bytes are an error.
    assert_eq!(decode(&[0x01, 0x02]).unwrap_err(), CborError::TrailingBytes { at: 1 });
    // A floating point head is not accepted.
    assert!(matches!(decode(&[0xf9, 0x00, 0x00]).unwrap_err(), CborError::UnsupportedHead { .. }));
}

#[test]
fn the_diagnostic_notation_reads_as_published() {
    let value = Cbor::Map(vec![
        (text("b"), Cbor::Array(vec![Cbor::Uint(2), Cbor::Nint(0)])),
        (text("a"), text("q\"x")),
        (Cbor::Uint(1), Cbor::Bytes(vec![0xde, 0xad])),
    ]);
    assert_eq!(diagnostic(&value), "{1: h'dead', \"a\": \"q\\\"x\", \"b\": [2, -1]}");
}
