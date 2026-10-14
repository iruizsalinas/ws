use ws_core::proto::frame::{
    BINARY, CLOSE, CONTINUATION, Fail, Msg, PING, TEXT, apply_mask, close_payload, encode, encoded_len,
};
use ws_core::proto::reader::Reader;

const KEY: [u8; 4] = [1, 2, 3, 4];

fn frame(fin: bool, op: u8, payload: &[u8]) -> Vec<u8> {
    let mut f = encode(op, payload, Some(KEY));
    if !fin {
        f[0] &= 0x7F;
    }
    f
}

fn read(server: bool, bytes: &[u8]) -> Result<Vec<Msg>, Fail> {
    let mut r = Reader::new(server, 1 << 20);
    r.push(bytes);
    let mut out = Vec::new();
    while let Some(m) = r.message()? {
        out.push(m);
    }
    Ok(out)
}

#[test]
fn roundtrip_lengths() {
    for len in [0, 1, 125, 126, 127, 65535, 65536, 100_000] {
        let data: Vec<u8> = (0..len).map(|i| i as u8).collect();
        assert_eq!(read(true, &encode(BINARY, &data, Some(KEY))).unwrap(), [Msg::Binary(data.clone())]);
        assert_eq!(read(false, &encode(BINARY, &data, None)).unwrap(), [Msg::Binary(data)]);
    }
}

#[test]
fn encoded_len_matches_encode() {
    for len in [0, 1, 125, 126, 127, 65535, 65536, 100_000] {
        let data = vec![0; len];
        assert_eq!(encoded_len(len, true), encode(BINARY, &data, Some(KEY)).len(), "masked {len}");
        assert_eq!(encoded_len(len, false), encode(BINARY, &data, None).len(), "unmasked {len}");
    }
}

#[test]
fn byte_by_byte() {
    let mut bytes = frame(false, TEXT, "h\u{e9}".as_bytes());
    bytes.extend(frame(true, PING, b"p"));
    bytes.extend(frame(true, CONTINUATION, "llo \u{1F600}".as_bytes()));
    let mut r = Reader::new(true, 1 << 20);
    let mut out = Vec::new();
    for b in bytes {
        r.push(&[b]);
        while let Some(m) = r.message().unwrap() {
            out.push(m);
        }
    }
    assert_eq!(out, [Msg::Ping(b"p".to_vec()), Msg::Text("h\u{e9}llo \u{1F600}".as_bytes().to_vec())]);
}

#[test]
fn utf8_split_across_fragments() {
    let s = "\u{1F600}".as_bytes();
    let bytes = [frame(false, TEXT, &s[..1]), frame(false, CONTINUATION, &s[1..3]), frame(true, CONTINUATION, &s[3..])]
        .concat();
    assert_eq!(read(true, &bytes).unwrap(), [Msg::Text(s.to_vec())]);
}

#[test]
fn continuation_assembly_unmasks_each_frame_once_and_keeps_controls_separate() {
    use ws_core::proto::frame::PONG;
    let text = "a\u{1f600}bc\u{e9}".repeat(100).into_bytes();
    let mut wire = Vec::new();
    let mut expected = Vec::new();
    for (i, part) in text.chunks(3).enumerate() {
        let op = if i == 0 { TEXT } else { CONTINUATION };
        let mut encoded = encode(op, part, Some([i as u8, 91, 37, 255]));
        if (i + 1) * 3 < text.len() {
            encoded[0] &= 0x7f;
        }
        wire.extend(encoded);
        if i % 11 == 0 {
            let ping = vec![i as u8];
            wire.extend(encode(PING, &ping, Some(KEY)));
            wire.extend(encode(PONG, &[], Some(KEY)));
            expected.extend([Msg::Ping(ping), Msg::Pong(vec![])]);
        }
    }
    expected.push(Msg::Text(text));
    let mut reader = Reader::new(true, 4096);
    let mut received = Vec::new();
    for chunk in wire.chunks(5) {
        reader.push(chunk);
        while let Some(msg) = reader.message().unwrap() {
            received.push(msg);
        }
        assert_eq!(reader.message().unwrap(), None, "polling incomplete input must not unmask again");
    }
    assert_eq!(received, expected);
}

#[test]
fn violations() {
    let mut rsv = frame(true, TEXT, b"x");
    rsv[0] |= 0x40;
    let mut long_len = encode(BINARY, &[0; 200], Some(KEY));
    long_len[1] = 0x80 | 127;
    let cases: Vec<(Vec<u8>, u16)> = vec![
        (rsv, 1002),
        (frame(true, 0x3, b""), 1002),
        (encode(TEXT, b"x", None), 1002),
        (frame(false, PING, b""), 1002),
        (frame(true, PING, &[0; 126]), 1002),
        (frame(true, CONTINUATION, b"x"), 1002),
        ([frame(false, TEXT, b"a"), frame(true, TEXT, b"b")].concat(), 1002),
        (frame(true, TEXT, &[0xFF]), 1007),
        (frame(false, TEXT, &[0xC0, 0x80]), 1007),
        ([frame(false, TEXT, &[0xF0, 0x9F]), frame(true, CONTINUATION, b"")].concat(), 1007),
        (frame(true, CLOSE, &[3]), 1002),
        (frame(true, CLOSE, &close_payload(1005, "")), 1002),
        (frame(true, CLOSE, &close_payload(2000, "")), 1002),
        (frame(true, CLOSE, &[0x03, 0xE8, 0xFF]), 1007),
        (long_len, 1009),
    ];
    for (bytes, code) in cases {
        assert_eq!(read(true, &bytes).unwrap_err().0, code, "{bytes:?}");
    }
    assert_eq!(read(false, &frame(true, TEXT, b"x")).unwrap_err().0, 1002);
}

#[test]
fn size_limit_checked_before_payload() {
    let mut r = Reader::new(true, 10);
    r.push(&frame(true, BINARY, &[0; 11])[..8]);
    assert_eq!(r.message().unwrap_err(), Fail(1009, "message too big"));
    let mut r = Reader::new(true, 10);
    r.push(&[frame(false, BINARY, &[0; 6]), frame(true, CONTINUATION, &[0; 6])].concat());
    assert_eq!(r.message().unwrap_err().0, 1009);
}

#[test]
fn close_frames() {
    assert_eq!(read(true, &frame(true, CLOSE, b"")).unwrap(), [Msg::Close(None)]);
    let c = frame(true, CLOSE, &close_payload(4000, "bye"));
    assert_eq!(read(true, &c).unwrap(), [Msg::Close(Some((4000, "bye".into())))]);
}

#[test]
fn mask_is_involution() {
    let mut d: Vec<u8> = (0..37).collect();
    apply_mask(&mut d, KEY);
    apply_mask(&mut d, KEY);
    assert_eq!(d, (0..37).collect::<Vec<u8>>());
}

#[test]
fn invalid_lengths_rejected_from_header() {
    for bytes in [
        vec![0x82, 126, 0, 125],
        vec![0x82, 127, 0, 0, 0, 0, 0, 0, 0xff, 0xff],
        vec![0x82, 127, 0x80, 0, 0, 0, 0, 0, 0, 0],
        vec![0x82, 127, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
    ] {
        let mut r = Reader::new(false, usize::MAX);
        r.push(&bytes);
        assert_eq!(r.message().unwrap_err().0, 1002, "{bytes:?}");
    }
}

#[test]
fn invalid_fragment_sequence_rejected_before_payload() {
    let mut r = Reader::new(false, 1024);
    r.push(&[0x80, 125]);
    assert_eq!(r.message().unwrap_err().0, 1002);
    let mut r = Reader::new(false, 1024);
    r.push(&[0x01, 0, 0x82, 125]);
    assert_eq!(r.message().unwrap_err().0, 1002);
}

#[test]
fn mutated_streams_are_independent_of_tcp_chunking() {
    let seed = [
        frame(false, TEXT, b"hello "),
        frame(true, PING, b"check"),
        frame(true, CONTINUATION, "\u{1f600}".as_bytes()),
        frame(true, BINARY, &[42; 257]),
        frame(true, CLOSE, &close_payload(1000, "done")),
    ]
    .concat();
    let mut state = 0x6455_u64;
    let mut random = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state as usize
    };
    for case in 0..4000 {
        let mut bytes = seed.clone();
        for _ in 0..case % 4 {
            let at = random() % bytes.len();
            bytes[at] ^= 1 << (random() % 8);
        }
        if case % 5 == 0 {
            bytes.truncate(random() % bytes.len());
        }
        let mut reader = Reader::new(true, 1 << 20);
        let mut messages = Vec::new();
        let chunked = (|| -> Result<Vec<Msg>, Fail> {
            let mut at = 0;
            while at < bytes.len() {
                let end = (at + 1 + random() % 31).min(bytes.len());
                reader.push(&bytes[at..end]);
                while let Some(msg) = reader.message()? {
                    messages.push(msg);
                }
                at = end;
            }
            Ok(messages)
        })();
        assert_eq!(chunked, read(true, &bytes), "mutation {case}");
    }
}
