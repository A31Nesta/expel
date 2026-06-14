use allocator_api2::collections::TryReserveError;

use crate::elf::error::ExpelError;

pub fn has_flag(flags: u32, flag: u32) -> bool {
    flags & flag == flag
}

pub fn elf_align(size: u32, align_size: u32) -> u32 {
    (size + (align_size - 1)) & !(align_size - 1)
}

// Function to map Memory Reservation errors to `MemoryFuckup` errors
pub fn map_mem_err(
    error: TryReserveError,
    capacity_overflow_msg: &'static str,
    alloc_error_msg: &'static str,
) -> ExpelError {
    match error.kind() {
        allocator_api2::collections::TryReserveErrorKind::CapacityOverflow => {
            ExpelError::MemoryFuckup(capacity_overflow_msg)
        }
        allocator_api2::collections::TryReserveErrorKind::AllocError {
            layout: _layout,
            non_exhaustive: _non_exhaustive,
        } => ExpelError::MemoryFuckup(alloc_error_msg),
    }
}
