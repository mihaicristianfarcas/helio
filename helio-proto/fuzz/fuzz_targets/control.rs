#![no_main]
use libfuzzer_sys::fuzz_target;
use helio_proto::control::Control;

// The control parser reads hostile network input: anything that reaches the
// tunnel can be handed to it (v2 design §10.1). It must never panic.
//
// Control bodies have fields and variable length: there are length checks and
// enum range checks that can get the bounds wrong.
fuzz_target!(|data: &[u8]| {
    let _ = Control::decode(data);
});
