#![no_std]

extern crate alloc;

mod dlso;
mod elf;

// API
pub use elf::ElfMain;
pub use elf::{elf_relocate, elf_request};
