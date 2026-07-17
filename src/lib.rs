#![no_std]

extern crate alloc;

mod dlso;
mod elf;

// API
pub use elf::{Elf, ElfEntry};
pub use elf::{elf_register_symbol, elf_relocate, elf_request, elf_unregister_symbol};
