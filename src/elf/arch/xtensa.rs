use allocator_api2::alloc::Allocator;
use defmt::{error, info, panic};
use elf::{relocation::Rela, symbol::Symbol};

use crate::elf::{elf_map_sym, types::Elf};

#[derive(PartialEq)]
pub struct Relocation(pub u32);

#[allow(dead_code)]
impl Relocation {
    pub const NONE: Self = Self(0);
    pub const RELOC_32: Self = Self(1);
    pub const RTLD: Self = Self(2);
    pub const GLOB_DAT: Self = Self(3);
    pub const JMP_SLOT: Self = Self(4);
    pub const RELATIVE: Self = Self(5);
    pub const PLT: Self = Self(6);
    pub const OP0: Self = Self(8);
    pub const OP1: Self = Self(9);
    pub const OP2: Self = Self(10);
    pub const ASM_EXPAND: Self = Self(11);
    pub const ASM_SIMPLIFY: Self = Self(12);
    pub const GNU_VTINHERIT: Self = Self(15);
    pub const GNU_VTENTRY: Self = Self(16);
    pub const DIFF8: Self = Self(17);
    pub const DIFF16: Self = Self(18);
    pub const DIFF32: Self = Self(19);
    pub const SLOT0_OP: Self = Self(20);
    pub const SLOT1_OP: Self = Self(21);
    pub const SLOT2_OP: Self = Self(22);
    pub const SLOT3_OP: Self = Self(23);
    pub const SLOT4_OP: Self = Self(24);
    pub const SLOT5_OP: Self = Self(25);
    pub const SLOT6_OP: Self = Self(26);
    pub const SLOT7_OP: Self = Self(27);
    pub const SLOT8_OP: Self = Self(28);
    pub const SLOT9_OP: Self = Self(29);
    pub const SLOT10_OP: Self = Self(30);
    pub const SLOT11_OP: Self = Self(31);
    pub const SLOT12_OP: Self = Self(32);
    pub const SLOT13_OP: Self = Self(33);
    pub const SLOT14_OP: Self = Self(34);
    pub const SLOT0_ALT: Self = Self(35);
    pub const SLOT1_ALT: Self = Self(36);
    pub const SLOT2_ALT: Self = Self(37);
    pub const SLOT3_ALT: Self = Self(38);
    pub const SLOT4_ALT: Self = Self(39);
    pub const SLOT5_ALT: Self = Self(40);
    pub const SLOT6_ALT: Self = Self(41);
    pub const SLOT7_ALT: Self = Self(42);
    pub const SLOT8_ALT: Self = Self(43);
    pub const SLOT9_ALT: Self = Self(44);
    pub const SLOT10_ALT: Self = Self(45);
    pub const SLOT11_ALT: Self = Self(46);
    pub const SLOT12_ALT: Self = Self(47);
    pub const SLOT13_ALT: Self = Self(48);
    pub const SLOT14_ALT: Self = Self(49);
}

/// _sym is unused because it was originally like that.
/// TODO: Decide what to do with this unused variable that also existed in the original
pub fn elf_arch_relocate<I>(
    elf: &mut Elf<I>,
    rela: &Rela,
    _sym: &Symbol,
    addr: u32,
    base_address: u32,
) where
    I: Allocator,
{
    // Get the address of the relocation in the actual memory
    let rela_addr = (base_address + rela.r_offset as u32) as *mut u32; // elf_map_sym(elf, rela.r_offset as u32) as *mut u32;

    info!("rela.r_offset = {:#x}", rela.r_offset);
    info!("rela_addr     = {:#x}", rela_addr);
    info!("literal.addr  = {:#x}", elf.sections.literal.addr);

    // Manage the relocation depending on its type
    match Relocation(rela.r_type) {
        Relocation::RELATIVE => {
            let val = elf_map_sym(elf, unsafe { *rela_addr });
            #[cfg(feature = "cache-offset")]
            unsafe {
                *rela_addr = compile_error!(
                    "Xtensa Relocation: Cache Offset feature is not supported - `elf_remap_text` function is required but not implemented"
                );
            }
            #[cfg(not(feature = "cache-offset"))]
            unsafe {
                *rela_addr = val;
            }
        }
        Relocation::RTLD => ( /* We don't have to do anything here :) */ ),
        Relocation::GLOB_DAT | Relocation::JMP_SLOT => {
            #[cfg(feature = "cache-offset")]
            unsafe {
                *rela_addr = compile_error!(
                    "Xtensa Relocation: Cache Offset feature is not supported - `elf_remap_text` function is required but not implemented"
                );
            }
            #[cfg(not(feature = "cache-offset"))]
            unsafe {
                *rela_addr = addr;
            }
        }
        Relocation::RELOC_32 => unsafe {
            info!("-- Relocating R_XTENSA_32");

            let final_val = addr.wrapping_add(rela.r_addend as u32);
            *rela_addr = final_val;

            info!("-- Relocated; Final Value: `{:#x}`", final_val);
        },
        Relocation::SLOT0_OP => unsafe {
            info!("-- Relocating R_XTENSA_SLOT0_OP");

            // Implementation ported from here:
            // - https://github.com/niicoooo/esp32-elfloader/blob/52531c631f1c723e6931966170f0b20ef0efa6db/components/elfloader/loader.c#L188
            let v = read_unaligned_32(rela_addr);
            let sym_addr = addr.wrapping_add(rela.r_addend as u32);

            info!("unpatched instruction = {:#010x}", v);

            // L32R
            if (v & 0x00000F) == 0x000001 {
                info!("---- Opcode format: L32R");

                let delta_raw: i32 = sym_addr as i32 - ((rela_addr.addr() + 3) & !0x3) as i32;
                if delta_raw & 0x3 > 0 {
                    error!("Relocation: L32R error");
                    panic!("Relocation: L32R error");
                }
                let delta = (delta_raw >> 2) as u32;

                // Patch the bytes of `v` with delta
                let mut v_bytes = v.to_le_bytes();
                let delta_bytes = delta.to_le_bytes();
                v_bytes[1] = delta_bytes[0];
                v_bytes[2] = delta_bytes[1];
                let patched = u32::from_le_bytes(v_bytes);

                info!("patched instruction = {:#010x}", patched);

                // Write
                write_unaligned_32(rela_addr, patched);

                info!("symbol = {:#010x}", sym_addr);
                info!("place  = {:#010x}", rela_addr.addr());
                info!("delta_raw = {:#010x}", delta_raw);
                info!("delta = {:#010x}", delta);

                info!("-- Relocated; Final Value: `{:#x}`", patched);
            }

            // CALL0, CALL4, CALL8, CALL12, J
            // TODO: Implement these if necessary
        },
        // TODO: Change this into an error type
        Relocation(unknown) => {
            error!("Unsupported Relocation type with ID: {}", unknown);
        }
    }
}

#[inline(always)]
pub unsafe fn read_unaligned_32(ptr: *const u32) -> u32 {
    let base_addr = (ptr.addr() & !0x3) as *const u32;
    let offset = (ptr.addr() & 0x3) as u32;

    let low_word = unsafe { *base_addr };
    let high_word = unsafe { *base_addr.add(1) };
    let combined = ((high_word as u64) << 32) | (low_word as u64);

    // Get the section that we need
    (combined >> (offset * 8)) as u32
}

#[inline(always)]
pub unsafe fn write_unaligned_32(ptr: *mut u32, value: u32) {
    let base_addr = (ptr.addr() & !0x3) as *mut u32;
    let offset = (ptr.addr() & 0x3) as u32;

    let low_word = unsafe { *base_addr };
    let high_word = unsafe { *base_addr.add(1) };
    let mut combined = ((high_word as u64) << 32) | (low_word as u64);

    // Write value into `combined`, then write that u64 into `base_addr`
    combined &= !((0xFFFFFFFFu64) << (offset * 8));
    combined |= (value as u64) << (offset * 8);

    unsafe {
        *base_addr = combined as u32;
        *base_addr.add(1) = (combined >> 32) as u32;
    };
}
