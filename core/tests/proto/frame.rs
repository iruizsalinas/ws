use ws_core::proto::frame::{BINARY, CLOSE, CONTINUATION, Fail, Msg, PING, TEXT, apply_mask, close_payload, encode};
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
