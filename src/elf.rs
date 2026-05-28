//! # Amogus

mod error;
mod types;

use elf::{
    ElfBytes,
    abi::{SHF_ALLOC, SHF_EXECINSTR, SHF_WRITE, SHT_NOBITS, SHT_PROGBITS},
    endian::NativeEndian,
};

use crate::elf::{error::ExpelError, types::ElfSections};

fn has_flag(flags: u32, flag: u32) -> bool {
    flags & flag == flag
}

fn elf_align(size: u32, align_size: u32) -> u32 {
    (size + (align_size - 1)) & !(align_size - 1)
}

fn elf_load_section(pbuf: &[u8]) -> Result<(), ExpelError> {
    let elf_file = ElfBytes::<NativeEndian>::minimal_parse(pbuf)
        .map_err(|e| ExpelError::ParseError("Shit didn't parse"))?;

    // Parse, we copy this block from `ElfFile::section_header_by_name` since it does what we want in this case lmao
    let (shdrs, strtab) = match elf_file
        .section_headers_with_strtab()
        .map_err(|e| ExpelError::ParseError("Couldn't parse Section Headers / String Table"))?
    {
        (Some(shdrs), Some(strtab)) => (shdrs, strtab),
        _ => {
            // If we don't have shdrs, or don't have a strtab, we can't find a section by its name
            return Err(ExpelError::NoShdrsStrtab);
        }
    };

    // Create the Elf Sections object
    let mut sections = ElfSections::default();

    // Read the headers to get the information needed to fill the `sections` object
    shdrs.iter().for_each(|shdr| {
        let name = strtab.get(shdr.sh_name as usize).unwrap_or("");
        let flags = shdr.sh_flags as u32;

        if shdr.sh_type == SHT_PROGBITS && has_flag(flags, SHF_ALLOC) {
            // Get data of the `.text` section
            if has_flag(flags, SHF_EXECINSTR) && name == ".text" {
                sections.text.v_addr = shdr.sh_addr as u32;
                sections.text.size = elf_align(shdr.sh_size as u32, 4);
                sections.text.offset = shdr.sh_offset as u32;
            }
            // `.data` section
            else if has_flag(flags, SHF_WRITE) && name == ".data" {
                sections.data.v_addr = shdr.sh_addr as u32;
                sections.data.size = shdr.sh_size as u32;
                sections.data.offset = shdr.sh_offset as u32;
            }
            // `.rodata` section
            else if name == ".rodata" {
                sections.rodata.v_addr = shdr.sh_addr as u32;
                sections.rodata.size = shdr.sh_size as u32;
                sections.rodata.offset = shdr.sh_offset as u32;
            }
            // `.data.rel.ro` section
            else if name == ".data.rel.ro" {
                sections.data_rel_ro.v_addr = shdr.sh_addr as u32;
                sections.data_rel_ro.size = shdr.sh_size as u32;
                sections.data_rel_ro.offset = shdr.sh_offset as u32;
            }
        }
        // `.bss` section
        else if shdr.sh_type == SHT_NOBITS
            && has_flag(flags, SHF_ALLOC | SHF_WRITE)
            && name == ".bss"
        {
            sections.bss.v_addr = shdr.sh_addr as u32;
            sections.bss.size = shdr.sh_size as u32;
            sections.bss.offset = shdr.sh_offset as u32;
        }
    });

    // The mallocs :(
    // TODO: Make the calls to `malloc` optional, make `alloc` optional. Separate this into low-level API and high-level API. In low-level, the function returns a request of what to allocate, in high-level, it allocates directly.
    // For now, let's use the mallocs directly, we want things to work, we'll fix things up later

    if sections.text.size == 0 {
        return Err(ExpelError::NoTextSection);
    }

    // Malloc here

    // calc size here

    // another malloc

    todo!()
}
