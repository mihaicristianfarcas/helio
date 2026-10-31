#![no_main]
use libfuzzer_sys::fuzz_target;
use helio_proto::clip;

// Clipboard messages and the copy they carry arrive from the peer over the
// tunnel. Neither parser may panic: `decode` slices at fixed offsets (a short
// ack once did), and `decode_items` walks lengths the peer chose.
fuzz_target!(|data: &[u8]| {
    let _ = clip::decode(data);
    let _ = clip::decode_items(data);
});
