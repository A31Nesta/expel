//! # Amogus

mod error;
mod types;

mod symbol;
mod util;

mod arch;

use defmt::{error, info, warn};
pub use symbol::{elf_register_symbol, elf_unregister_symbol};
pub use types::ElfMain;

use core::{
    ffi::{c_char, c_int},
    mem::transmute,
    ptr::{copy_nonoverlapping, null},
};

use alloc::{ffi::CString, vec::Vec};
use allocator_api2::alloc::Allocator;
use elf::{
    ElfBytes,
    abi::{SHF_ALLOC, SHF_EXECINSTR, SHF_WRITE, SHT_NOBITS, SHT_PROGBITS, SHT_RELA},
    endian::NativeEndian,
    section::{SectionHeader, SectionHeaderTable},
    string_table::StringTable,
    symbol::SymbolTable,
};

use crate::elf::{
    arch::elf_arch_relocate,
    error::ExpelError,
    symbol::elf_find_symbol,
    types::Elf,
    util::{elf_align, has_flag, map_mem_err},
};

fn elf_load_section<I>(
    elf_file: &ElfBytes<NativeEndian>,
    pbuf: &[u8],
    iram_alloc: I,
) -> Result<Elf<I>, ExpelError>
where
    I: Allocator,
{
    let mut elf = Elf::empty(iram_alloc);

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
    shdrs.iter().enumerate().for_each(|(index, shdr)| {
        let name = strtab.get(shdr.sh_name as usize).unwrap_or("");
        let flags = shdr.sh_flags as u32;

        if shdr.sh_type == SHT_PROGBITS
        /* && has_flag(flags, SHF_ALLOC) */
        {
            // Get data of the `.text` section
            if has_flag(flags, SHF_EXECINSTR) && name == ".text" {
                elf.sections.text.v_addr = shdr.sh_addr as u32;
                elf.sections.text.size = elf_align(shdr.sh_size as u32, 4);
                elf.sections.text.offset = shdr.sh_offset as u32;
                elf.sections.text.index = index as u32;
                warn!("[TEXT] SECTION INDEX = {}", index);
            }
            // `.literal` section
            else if name == ".literal" || name == ".xt.lit" {
                elf.sections.literal.v_addr = shdr.sh_addr as u32;
                elf.sections.literal.size = elf_align(shdr.sh_size as u32, 4); // shdr.sh_size as u32;
                elf.sections.literal.offset = shdr.sh_offset as u32;
                elf.sections.literal.index = index as u32;
                warn!("[LITERAL] SECTION INDEX = {}", index);
            }
            // `.data` section
            else if has_flag(flags, SHF_WRITE) && name == ".data" {
                elf.sections.data.v_addr = shdr.sh_addr as u32;
                elf.sections.data.size = shdr.sh_size as u32;
                elf.sections.data.offset = shdr.sh_offset as u32;
                elf.sections.data.index = index as u32;
                warn!("[DATA] SECTION INDEX = {}", index);
            }
            // `.rodata` section. Rust _loves_ outputting several of these
            // ( none of which is actually called `.rodata` :) )
            else if name == ".rodata" {
                elf.sections.rodata.v_addr = shdr.sh_addr as u32;
                elf.sections.rodata.size = shdr.sh_size as u32;
                elf.sections.rodata.offset = shdr.sh_offset as u32;
                elf.sections.rodata.index = index as u32;
                warn!("[RODATA] SECTION INDEX = {}", index);
            }
            // `.data.rel.ro` section
            else if name == ".data.rel.ro" {
                elf.sections.data_rel_ro.v_addr = shdr.sh_addr as u32;
                elf.sections.data_rel_ro.size = shdr.sh_size as u32;
                elf.sections.data_rel_ro.offset = shdr.sh_offset as u32;
                elf.sections.data_rel_ro.index = index as u32;
                warn!("[DATA.REL.RO] SECTION INDEX = {}", index);
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
            warn!("[BSS] SECTION INDEX = {}", index);
        }
    });

    if elf.sections.text.size == 0 {
        error!("Oh shit! No Text?");
        return Err(ExpelError::NoTextSection);
    }

    // Malloc here
    let text_block_count = ((elf.sections.text.size + elf.sections.literal.size) as usize + 3) / 4;
    elf.ptext.try_reserve(text_block_count).map_err(|e| {
        map_mem_err(
            e,
            "Attempted to allocate more IRAM than the maximum capacity", // When we exceed maximum capacity
            "Allocation error for IRAM", // When there's an error during allocation
        )
    })?;

    // calc size here
    let data_size = elf.sections.data.size
        + elf.sections.rodata.size
        + elf.sections.bss.size
        + elf.sections.data_rel_ro.size;

    // another malloc
    // TODO: Check if `data_size` is more than 0
    elf.pdata.try_reserve(data_size as usize).map_err(|_| {
        ExpelError::MemoryFuckup("Error while reserving on the Global allocator... oops")
    })?;

    info!("Allocated IRAM and DRAM buffers");

    // memcpy `.text`
    // ==============

    // - Update the address of .text to point to the new buffer.
    // By the way yes I comment a lot but I'm not a clanker, I just use comments to take notes and learn while porting the C code to Rust
    let mut ptext = elf.ptext.as_mut_ptr();
    elf.sections.text.addr = ptext.addr() as u32;

    // TODO: Make sure everything is aligned first! We don't manually align `program_bytes` so it might not always work
    unsafe {
        copy_nonoverlapping(
            pbuf.as_ptr().byte_offset(elf.sections.text.offset as isize) as *const u32,
            ptext,
            (elf.sections.text.size as usize + 3) / 4,
        );
        ptext = ptext.offset((elf.sections.text.size as isize + 3) / 4);

        info!(
            "Copied .text - Copying literal ({} bytes)",
            elf.sections.literal.size
        );

        // .literal
        if elf.sections.literal.size > 0 {
            elf.sections.literal.addr = ptext.addr() as u32;
            info!(
                "Set .literal addr - Copying {} packets from address {:#x} | Buffer capacity: {} - Total used capacity: {}",
                (elf.sections.literal.size as usize + 3) / 4,
                elf.sections.literal.addr,
                text_block_count,
                (elf.sections.text.size as usize + 3) / 4
                    + (elf.sections.literal.size as usize + 3) / 4
            );
            warn!(
                "Base address: {:#x} | Offset by packets: {} | Updated address: {:#x} | Expected end: {:#x} | Allocated end: {:#x}",
                elf.ptext.as_ptr().addr(),
                (elf.sections.text.size as isize + 3) / 4,
                ptext.addr(),
                ptext.addr() + (((elf.sections.literal.size as usize + 3) / 4) * 4),
                elf.ptext.as_ptr().addr()
                    + (((elf.sections.text.size as usize + 3) / 4
                        + (elf.sections.literal.size as usize + 3) / 4)
                        * 4),
            );
            let mut intermediate: Vec<u32> =
                Vec::with_capacity((elf.sections.literal.size as usize + 3) / 4);
            copy_nonoverlapping(
                pbuf.as_ptr()
                    .byte_offset(elf.sections.literal.offset as isize) as *const u8,
                intermediate.as_mut_ptr() as *mut u8,
                ((elf.sections.literal.size as usize + 3) / 4) * 4,
            );
            copy_nonoverlapping(
                intermediate.as_ptr(),
                ptext,
                (elf.sections.literal.size as usize + 3) / 4,
            );
            info!("Copied .literal");
        }
        elf.ptext.set_len(text_block_count);
    }

    info!("Copied IRAM sections");

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

    info!("Copied DRAM sections");

    // DEBUG
    info!(".text runtime addr: {:#x}", elf.sections.text.addr);
    info!(".literal runtime addr: {:#x}", elf.sections.literal.addr);

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
        elf.entry = Some(entry as *mut u8);
    }

    Ok(elf)
}

/// Obtains the Symbol Table and String Table for the section header and
/// returns them as instances of SymbolTable and StringTable.
fn symtab_with_strtab_for_shdr<'a>(
    elf_file: &'a ElfBytes<NativeEndian>,
    section_headers: SectionHeaderTable<NativeEndian>,
    shdr: &'_ SectionHeader,
) -> (SymbolTable<'a, NativeEndian>, StringTable<'a>) {
    // Get symbol table
    let symtab_shdr = section_headers
        .get(shdr.sh_link as usize)
        .expect("Couldn't get RELA symtab shdr");
    let symtab_data = elf_file
        .section_data(&symtab_shdr)
        .expect("Couldn't get RELA symtab data");
    let symtab = SymbolTable::new(NativeEndian, elf_file.ehdr.class, &symtab_data.0);

    // Get string table
    let strtab_shdr = section_headers
        .get(symtab_shdr.sh_link as usize)
        .expect("Couldn't get RELA strtab shdr");
    let strtab = elf_file
        .section_data_as_strtab(&strtab_shdr)
        .expect("Couldn't parse RELA strtab");

    (symtab, strtab)
}

/// Decode and relocate ELF data
pub fn elf_relocate<I>(pbuf: &[u8], iram_alloc: I) -> Result<Elf<I>, ExpelError>
where
    I: Allocator,
{
    // Parse the ELF file from `pbuf` and obtain the `ElfBytes` object
    let elf_file = ElfBytes::<NativeEndian>::minimal_parse(pbuf)
        .map_err(|_| ExpelError::ParseError("Shit didn't parse"))?;

    // Get the `elf` object that contains allocated `text` and `data` buffers and section information
    #[cfg(feature = "bus-address-mirror")]
    let mut elf = elf_load_section(&elf_file, pbuf, iram_alloc)?;
    #[cfg(not(feature = "bus-address-mirror"))]
    let elf = compile_error!(
        "elf_relocate: `bus-address-mirror` feature must be enabled. `elf_load_segment` function is not implemented in this version"
    );

    let section_headers = elf_file
        .section_headers()
        .expect("Parsing error on Section Headers");

    // Get section with `SHT_RELA` type and do the stuff(TM) with all the relocations
    // TODO: When implementing DLSO, remove the filter and change it into a check (`if`) inside the loop
    for shdr in section_headers
        .iter()
        .filter(|shdr| shdr.sh_type == SHT_RELA)
    {
        // Get relocations
        let relas = elf_file
            .section_data_as_relas(&shdr)
            .inspect_err(|_| error!("Parsing error on Section Data as RELAs"))
            .unwrap();

        // Get Symtab and Strtab
        let (symtab, strtab) = symtab_with_strtab_for_shdr(&elf_file, section_headers, &shdr);

        // Relocation target section and address
        let target_section = elf
            .sections
            .into_iter()
            .find(|sec| shdr.sh_info == sec.index);
        if target_section.is_none() {
            error!(
                "Couldn't obtain target for relocation section. Target section: {}",
                shdr.sh_info
            );
            panic!("Couldn't obtain target for relocation section");
        }
        let target_base = target_section.unwrap().addr;

        // Main loop over all relocations
        for rela in relas {
            let sym = symtab
                .get(rela.r_sym as usize)
                .inspect_err(|_| error!("Couldn't obtain symbol for RELA"))
                .unwrap();

            let r_type = rela.r_type as u8;
            let name = strtab
                .get(sym.st_name as usize)
                .inspect_err(|_| error!("Couldn't parse string from strtab"))
                .unwrap();

            info!(
                "RELOCATING TYPE: {} | NAME OF SYMBOL: {}",
                rela.r_type, name
            );

            let addr = if r_type == 5  /* R_XTENSA_RELATIVE */ ||
                          r_type == 1  /* R_XTENSA_32 */       ||
                          r_type == 3
            /* R_XTENSA_GLOB_DAT */
            {
                // Name can be empty, we skip those cases
                if name.is_empty() {
                    None
                }
                // If the name is not empty, we actually do something lol
                else {
                    let addr = elf_find_symbol(name, Some(&elf), Some(&sym));

                    // We prepare this for future (possible updates)
                    #[cfg(feature = "dlso")]
                    {
                        compile_error!("elf_relocate: DLSO is not yet supported");
                    }

                    // TODO: Change check from `== 0` to `Result`
                    // TODO: Remove the panic!
                    if addr == 0 {
                        error!("Can't find dumbass symbol");
                        panic!("Can't find dumbass symbol");
                    }
                    Some(addr)
                }
            } else if r_type == 4 /* R_XTENSA_JMP_SLOT */ || r_type == 20
            /* R_XTENSA_SLOT0_OP */
            {
                let addr = if sym.st_value != 0 {
                    elf_map_sym(&elf, sym.st_value as u32) as usize
                } else {
                    elf_find_symbol(name, Some(&elf), Some(&sym))
                };

                // We prepare this for future (possible updates)
                #[cfg(feature = "dlso")]
                {
                    compile_error!("elf_relocate: DLSO is not yet supported");
                }

                // TODO: Change check from `== 0` to `Result`
                // TODO: Remove the panic!
                if addr == 0 {
                    error!("Can't find dumbass symbol");
                    panic!("Can't find dumbass symbol");
                }

                Some(addr)
            } else {
                None
            };

            if let Some(address) = addr {
                // info!("About to relocate: NAME: `{}` | ADDR: `{}`", name, address);
                elf_arch_relocate(&mut elf, &rela, &sym, address as u32, target_base);
            }
        }
    }

    // TODO: Add `psram` feature or config option for flush. Original C line:
    // esp_elf_arch_flush();

    Ok(elf)
}

/// Calls the main function using Rust String slices as arguments.
///
/// THIS REQUIRES A GLOBAL ALLOCATOR!!
pub fn elf_request<I>(elf: &Elf<I>, args: &[&str]) -> i32
where
    I: Allocator,
{
    if let Some(entry) = elf.entry {
        // Conversion to C Strings
        let mut argv: Vec<*const c_char> = args
            .iter()
            .map(|s| {
                let cstr = CString::new(*s).unwrap();
                cstr.as_ptr()
            })
            .collect();

        argv.push(null());

        // Ru-nning in the nine-ties
        let main: ElfMain = unsafe { transmute(entry) };
        main(args.len() as c_int, argv.as_ptr())
    } else {
        -1
    }
}

fn elf_map_sym<I>(elf: &Elf<I>, sym: u32) -> u32
where
    I: Allocator,
{
    for section in &elf.sections {
        if (sym >= section.v_addr) && (sym < (section.v_addr + section.size)) {
            return sym - section.v_addr + section.addr;
        }
    }

    0
}
