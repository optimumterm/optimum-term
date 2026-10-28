use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSize {
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PtyEvent {
    Output(Vec<u8>),
    Exited(Option<u32>),
}

pub trait Pty {
    fn write(&mut self, bytes: &[u8]) -> io::Result<()>;
    fn resize(&mut self, size: GridSize) -> io::Result<()>;
}
