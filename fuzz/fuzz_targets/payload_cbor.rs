#![no_main]
//! Fuzz CBOR decoding of a `Payload` — the inner, recipient-decrypted message
//! body (text, SOS, location, POI, strobe, group ops, …). Decoding attacker-
//! influenced bytes here must be panic- and allocation-safe.

use libfuzzer_sys::fuzz_target;
use lifeline_proto::codec::from_cbor;
use lifeline_proto::Payload;

fuzz_target!(|data: &[u8]| {
    let _ = from_cbor::<Payload>(data);
});
