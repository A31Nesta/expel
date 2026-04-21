use core::ptr::NonNull;

/// A raw buffer to bytes containing the loaded ELF file.
pub struct ElfFile {
    payload: Option<NonNull<u8>>,
    size: usize,
}

impl ElfFile {
    /// Prepares a new empty ELF file.
    pub fn new() -> Self {
        ElfFile {
            payload: None,
            size: 0,
        }
    }
}
