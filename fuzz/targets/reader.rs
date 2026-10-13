//! The frame reader never panics, and how TCP splits the bytes never changes
//! what it decodes.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ws_core::proto::frame::{Fail, Msg};
use ws_core::proto::reader::Reader;

fn decode<'a>(server: bool, chunks: impl Iterator<Item = &'a [u8]>) -> Result<Vec<Msg>, Fail> {
    let mut reader = Reader::new(server, 1 << 16);
    let mut out = Vec::new();
    for chunk in chunks {
        reader.push(chunk);
        while let Some(msg) = reader.message()? {
            out.push(msg);
        }
    }
    Ok(out)
}

fuzz_target!(|input: &[u8]| {
    let Some((&first, data)) = input.split_first() else { return };
    let server = first & 1 == 1;
    let size = usize::from(first >> 1) + 1;
    assert_eq!(decode(server, std::iter::once(data)), decode(server, data.chunks(size)));
});
