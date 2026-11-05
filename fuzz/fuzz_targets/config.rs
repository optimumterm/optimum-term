#![no_main]

use libfuzzer_sys::fuzz_target;
use optimum_core::{Config, parse_color};

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = Config::parse(text);
        let _ = parse_color(text);
    }
});
