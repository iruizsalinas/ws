use ws_core::proto::base64::{decode, encode};

#[test]
fn roundtrip() {
    for (raw, enc) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foobar", "Zm9vYmFy")] {
        assert_eq!(encode(raw.as_bytes()), enc);
        assert_eq!(decode(enc).unwrap(), raw.as_bytes());
    }
}

#[test]
fn rejects_malformed() {
    for bad in ["Zg=", "Zh==", "Zm9=", "Z===", "Zm9v!A==", "=Zm9", "Zg==Zg=="] {
        assert!(decode(bad).is_none(), "{bad}");
    }
}
