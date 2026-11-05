#![no_main]

use ghostty_vt::{RenderState, Terminal};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(mut terminal) = Terminal::new(80, 24) else {
        return;
    };
    let Ok(mut render) = RenderState::new() else {
        return;
    };
    for chunk in data.split(|&b| b == 0xFF) {
        terminal.write(chunk);
        let _ = render.update(&terminal);
        let _ = terminal.take_responses();
        let _ = terminal.take_clipboard();
        if chunk.len() >= 2 {
            let _ = terminal.resize(u16::from(chunk[0]) + 1, u16::from(chunk[1]) + 1, 8, 16);
        }
    }
    let _ = terminal.plain_text();
});
