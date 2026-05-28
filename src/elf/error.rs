use core::{error::Error, fmt::Display};

#[derive(Debug)]
pub enum ExpelError {
    ParseError(&'static str),
    NoShdrsStrtab,
    NoTextSection,
    MemoryFuckup(&'static str),
}

impl Display for ExpelError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ExpelError::ParseError(m) => write!(f, "Can't parse ELF File: {}", m),
            ExpelError::NoShdrsStrtab => {
                write!(f, "File doesn't contain Section Headers or String Table")
            }
            ExpelError::NoTextSection => write!(f, "File doesn't contain a `.text` section"),
            ExpelError::MemoryFuckup(m) => write!(f, "Memory Fuckup happened :( - {}", m),
        }
    }
}
impl Error for ExpelError {}
