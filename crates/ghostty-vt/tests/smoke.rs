use ghostty_vt::Terminal;

#[test]
fn plain_text_reflects_written_bytes() {
    let mut term = Terminal::new(80, 24).unwrap();
    term.write(b"Line 1: Hello World!\r\n");
    term.write(b"Line 2: \x1b[1mBold\x1b[0m and \x1b[4mUnderline\x1b[0m\r\n");
    term.write(b"Line 3: placeholder\r\n");
    term.write(b"\x1b[3;1H\x1b[2KLine 3: Overwritten!");
    term.write(b"\x1b[5;10HPlaced at (5,10)");

    let text = term.plain_text().unwrap();
    let lines: Vec<&str> = text.lines().collect();

    assert_eq!(lines[0], "Line 1: Hello World!");
    assert_eq!(lines[1], "Line 2: Bold and Underline");
    assert_eq!(lines[2], "Line 3: Overwritten!");
    assert_eq!(lines[3], "");
    assert_eq!(lines[4], "         Placed at (5,10)");
}

#[test]
fn utf8_and_wide_chars_round_trip() {
    let mut term = Terminal::new(20, 4).unwrap();
    term.write("привет 日本語".as_bytes());
    assert_eq!(
        term.plain_text().unwrap().lines().next(),
        Some("привет 日本語")
    );
}

#[test]
fn resize_reflows_soft_wrapped_line() {
    let mut term = Terminal::new(10, 4).unwrap();
    term.write(b"0123456789abcde");
    assert_eq!(
        term.plain_text()
            .unwrap()
            .lines()
            .take(2)
            .collect::<Vec<_>>(),
        ["0123456789", "abcde"]
    );

    term.resize(20, 4, 8, 16).unwrap();
    assert_eq!(
        term.plain_text().unwrap().lines().next(),
        Some("0123456789abcde")
    );
}

#[test]
fn invalid_size_is_an_error() {
    let mut term = Terminal::new(10, 4).unwrap();
    assert!(term.resize(0, 0, 8, 16).is_err());
}

#[test]
fn device_status_report_is_answered() {
    let mut term = Terminal::new(80, 24).unwrap();
    term.write(b"\x1b[3;5H\x1b[6n");
    assert_eq!(term.take_responses(), b"\x1b[3;5R");
    assert!(term.take_responses().is_empty());
}
