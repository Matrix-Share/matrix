#![no_main]
//! Fuzz CBOR decoding of a `Bundle` — the top-level wire object a node receives
//! from any peer. The decoder is bounded on purpose; arbitrary bytes must decode
//! to a value or a clean error, never a panic or unbounded allocation.

use libfuzzer_sys::fuzz_target;
use lifeline_proto::codec::from_cbor;
use lifeline_proto::Bundle;

fuzz_target!(|data: &[u8]| {
    let _ = from_cbor::<Bundle>(data);
});
