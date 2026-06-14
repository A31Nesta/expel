//! # Amogus

mod error;
mod types;

mod util;

use core::ptr::copy_nonoverlapping;

use allocator_api2::alloc::Allocator;
use elf::{
    ElfBytes,
    abi::{SHF_ALLOC, SHF_EXECINSTR, SHF_WRITE, SHT_NOBITS, SHT_PROGBITS},
    endian::NativeEndian,
};

use crate::elf::{
    error::ExpelError,
    types::{Elf, ElfMain},
    util::{elf_align, has_flag, map_mem_err},
};

fn elf_load_section<I, D>(
    pbuf: &[u8],
    data_alloc: D,
    iram_alloc: I,
) -> Result<Elf<I, D>, ExpelError>
where
    I: Allocator,
    D: Allocator,
{
    let mut elf = Elf::empty(iram_alloc, data_alloc);

    let elf_file = ElfBytes::<NativeEndian>::minimal_parse(pbuf)
        .map_err(|_| ExpelError::ParseError("Shit didn't parse"))?;

    // Parse, we copy this block from `ElfFile::section_header_by_name` since it does what we want in this case lmao
    let (shdrs, strtab) = match elf_file
        .section_headers_with_strtab()
        .map_err(|_| ExpelError::ParseError("Couldn't parse Section Headers / String Table"))?
    {
        (Some(shdrs), Some(strtab)) => (shdrs, strtab),
        _ => {
            // If we don't have shdrs, or don't have a strtab, we can't find a section by its name
            return Err(ExpelError::NoShdrsStrtab);
        }
    };

    // Read the headers to get the information needed to fill the `sections` object
    shdrs.iter().for_each(|shdr| {
        let name = strtab.get(shdr.sh_name as usize).unwrap_or("");
        let flags = shdr.sh_flags as u32;

        if shdr.sh_type == SHT_PROGBITS && has_flag(flags, SHF_ALLOC) {
            // Get data of the `.text` section
            if has_flag(flags, SHF_EXECINSTR) && name == ".text" {
                elf.sections.text.v_addr = shdr.sh_addr as u32;
                elf.sections.text.size = elf_align(shdr.sh_size as u32, 4);
                elf.sections.text.offset = shdr.sh_offset as u32;
            }
            // `.data` section
            else if has_flag(flags, SHF_WRITE) && name == ".data" {
                elf.sections.data.v_addr = shdr.sh_addr as u32;
                elf.sections.data.size = shdr.sh_size as u32;
                elf.sections.data.offset = shdr.sh_offset as u32;
            }
            // `.rodata` section
            else if name == ".rodata" {
                elf.sections.rodata.v_addr = shdr.sh_addr as u32;
                elf.sections.rodata.size = shdr.sh_size as u32;
                elf.sections.rodata.offset = shdr.sh_offset as u32;
            }
            // `.data.rel.ro` section
            else if name == ".data.rel.ro" {
                elf.sections.data_rel_ro.v_addr = shdr.sh_addr as u32;
                elf.sections.data_rel_ro.size = shdr.sh_size as u32;
                elf.sections.data_rel_ro.offset = shdr.sh_offset as u32;
            }
        }
        // `.bss` section
        else if shdr.sh_type == SHT_NOBITS
            && has_flag(flags, SHF_ALLOC | SHF_WRITE)
            && name == ".bss"
        {
            elf.sections.bss.v_addr = shdr.sh_addr as u32;
            elf.sections.bss.size = shdr.sh_size as u32;
            elf.sections.bss.offset = shdr.sh_offset as u32;
        }
    });

    // The mallocs :(
    // TODO: Make the calls to `malloc` optional, make `alloc` optional. Separate this into low-level API and high-level API. In low-level, the function returns a request of what to allocate, in high-level, it allocates directly.
    // For now, let's use the mallocs directly, we want things to work, we'll fix things up later

    if elf.sections.text.size == 0 {
        return Err(ExpelError::NoTextSection);
    }

    // Malloc here
    let text_block_count = (elf.sections.text.size as usize + 3) / 4;
    elf.ptext.try_reserve(text_block_count).map_err(|e| {
        map_mem_err(
            e,
            "Attempted to allocate more `.text` than the maximum capacity", // When we exceed maximum capacity
            "Allocation error for `.text`", // When there's an error during allocation
        )
    })?;

    // calc size here
    let data_size = elf.sections.data.size
        + elf.sections.rodata.size
        + elf.sections.bss.size
        + elf.sections.data_rel_ro.size;

    // another malloc
    // TODO: Check if `data_size` is more than 0
    elf.pdata.try_reserve(data_size as usize).map_err(|e| {
        map_mem_err(
            e,
            "Attempted to allocate more ELF data than the maximum capacity",
            "Allocation error for ELF data",
        )
    })?;

    // memcpy `.text`
    // ==============

    // - Update the address of .text to point to the new buffer.
    // By the way yes I comment a lot but I'm not a clanker, I just use comments to take notes and learn while porting the C code to Rust
    elf.sections.text.addr = elf.ptext.as_ptr().addr() as u32;

    // TODO: Make sure everything is aligned first! We don't manually align `program_bytes` so it might not always work
    let text_dest_ptr = elf.ptext.as_mut_ptr() as *mut u32;
    unsafe {
        copy_nonoverlapping(
            pbuf.as_ptr().byte_offset(elf.sections.text.offset as isize) as *const u32,
            text_dest_ptr,
            text_block_count,
        );
        elf.ptext.set_len(text_block_count);
    }

    // CONFIG_ELF_LOADER_SET_MMU
    #[cfg(feature = "set-mmu")]
    compile_error!(
        "`set-mmu` is an option for the ESP32-S2 when PSRAM is enabled, not implemented: ESP32-S2 support is not planned (can't test)"
    );

    // memcpy the rest
    // TODO: Check if size is more than 0
    let mut pdata = elf.pdata.as_mut_ptr();

    // .data
    if elf.sections.data.size > 0 {
        elf.sections.data.addr = pdata.addr() as u32;
        unsafe {
            copy_nonoverlapping(
                pbuf.as_ptr().offset(elf.sections.data.offset as isize),
                pdata,
                elf.sections.data.size as usize,
            );

            pdata = pdata.offset(elf.sections.data.size as isize);
        }
    }
    // .rodata
    if elf.sections.rodata.size > 0 {
        elf.sections.rodata.addr = pdata.addr() as u32;
        unsafe {
            copy_nonoverlapping(
                pbuf.as_ptr().offset(elf.sections.rodata.offset as isize),
                pdata,
                elf.sections.rodata.size as usize,
            );

            pdata = pdata.offset(elf.sections.rodata.size as isize);
        }
    }
    // .data_rel_ro
    if elf.sections.data_rel_ro.size > 0 {
        elf.sections.data_rel_ro.addr = pdata.addr() as u32;
        unsafe {
            copy_nonoverlapping(
                pbuf.as_ptr()
                    .offset(elf.sections.data_rel_ro.offset as isize),
                pdata,
                elf.sections.data_rel_ro.size as usize,
            );

            pdata = pdata.offset(elf.sections.data_rel_ro.size as isize);
        }
    }
    // .bss
    if elf.sections.bss.size > 0 {
        elf.sections.bss.addr = pdata.addr() as u32;
        unsafe {
            copy_nonoverlapping(
                pbuf.as_ptr().offset(elf.sections.bss.offset as isize),
                pdata,
                elf.sections.bss.size as usize,
            );
        }
    }

    // Set ELF Entry
    let entry = elf_file.ehdr.e_entry as u32 + elf.sections.text.addr - elf.sections.text.v_addr;

    #[cfg(feature = "cache-offset")]
    {
        elf.entry = compile_error!(
            "elf_remap_text is not implemented but is required to load to PSRAM on the ESP32-S3"
        );
    }
    #[cfg(not(feature = "cache-offset"))]
    {
        elf.entry = Some(entry as *mut ElfMain);
    }

    Ok(elf)
}
