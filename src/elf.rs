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

use alloc::{ffi::CString, string::ToString, vec::Vec};
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
    types::{Elf, ElfSection},
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

        if shdr.sh_type == SHT_PROGBITS {
            // Generic: Get all sections :)
            let mut section = ElfSection::new(
                index as u32,
                name.to_string(),
                shdr.sh_addr as u32,
                shdr.sh_offset as u32,
                0,
                shdr.sh_size as u32,
                false,
            );

            // Now we fine-tune. We can't find the real address yet, but we can figure
            // out if we have to align the size and if this should be placed in IRAM

            if has_flag(flags, SHF_EXECINSTR)
                // This below is a dirty trick because I don't know how to place `.literal` in IRAM in any other way
                // I want `.literal` in IRAM because I need its address to be close to the `.text` address for relocations
                || (name.starts_with(".literal") || name.starts_with(".xt.lit"))
            {
                section.size = elf_align(section.size, 4);
                section.iram = true;
            }

            elf.sections.add_section(section);

            warn!("[{}] Registered section - Index = {}", name, index);
        }
        // `.bss` section
        else if shdr.sh_type == SHT_NOBITS && has_flag(flags, SHF_ALLOC | SHF_WRITE) {
            let section = ElfSection::new(
                index as u32,
                name.to_string(),
                shdr.sh_addr as u32,
                shdr.sh_offset as u32,
                0,
                shdr.sh_size as u32,
                false,
            );

            elf.sections.add_section(section);
        }
    });

    // Get total sizes of buffers
    let mut iram_size: u32 = 0;
    let mut dram_size: u32 = 0;

    elf.sections.vec().iter().for_each(|sec| {
        if sec.iram {
            iram_size += sec.size
        } else {
            dram_size += sec.size
        }
    });

    let iram_block_count = (iram_size as usize + 3) / 4;
    elf.ptext.try_reserve(iram_block_count).map_err(|e| {
        map_mem_err(
            e,
            "Attempted to allocate more IRAM than the maximum capacity", // When we exceed maximum capacity
            "Allocation error for IRAM", // When there's an error during allocation
        )
    })?;

    // another malloc
    // TODO: Check if `data_size` is more than 0
    elf.pdata.try_reserve(dram_size as usize).map_err(|_| {
        ExpelError::MemoryFuckup("Error while reserving on the Global allocator... oops")
    })?;

    info!("Allocated IRAM and DRAM buffers");

    // memcpy IRAM data
    let mut ptext = elf.ptext.as_mut_ptr();

    for section in elf.sections.vec_mut().iter_mut().filter(|sec| sec.iram) {
        // We finally have the address of the sections!
        section.addr = ptext.addr() as u32;
        // We create an intermediate buffer that we can copy byte by byte to ensure 32-bit alignment
        let mut intermediate: Vec<u32> = Vec::with_capacity((section.size as usize + 3) / 4);
        unsafe {
            copy_nonoverlapping(
                pbuf.as_ptr().byte_offset(section.offset as isize) as *const u8,
                intermediate.as_mut_ptr() as *mut u8,
                ((section.size as usize + 3) / 4) * 4,
            );
            // Then we copy the intermediate buffer into the real one
            copy_nonoverlapping(
                intermediate.as_ptr(),
                ptext,
                (section.size as usize + 3) / 4,
            );

            // Now we update this for the next iteration
            ptext = ptext.offset((section.size as isize + 3) / 4);
        }
        info!("[{}] Section copied", section.name.as_str());
    }

    info!("Copied IRAM sections");

    // CONFIG_ELF_LOADER_SET_MMU
    #[cfg(feature = "set-mmu")]
    compile_error!(
        "`set-mmu` is an option for the ESP32-S2 when PSRAM is enabled, not implemented: ESP32-S2 support is not planned (can't test)"
    );

    // memcpy DRAM data
    let mut pdata = elf.pdata.as_mut_ptr();

    for section in elf.sections.vec_mut().iter_mut().filter(|sec| !sec.iram) {
        // We update the addr so that it has the real address:
        section.addr = pdata.addr() as u32;

        unsafe {
            copy_nonoverlapping(
                pbuf.as_ptr().offset(section.offset as isize),
                pdata,
                section.size as usize,
            );

            pdata = pdata.offset(section.size as isize);
        }
    }

    info!("Copied DRAM sections");

    // Set ELF Entry
    let entry = v_addr_to_addr(&elf, elf_file.ehdr.e_entry as u32).unwrap();

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
            .vec()
            .iter()
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

pub fn v_addr_to_addr<I>(elf: &Elf<I>, v_addr: u32) -> Option<u32>
where
    I: Allocator,
{
    elf.sections.vec().iter().find_map(|sec| {
        if v_addr >= sec.v_addr && v_addr < sec.v_addr + sec.size {
            Some(sec.addr + (v_addr - sec.v_addr))
        } else {
            None
        }
    })
}

fn elf_map_sym<I>(elf: &Elf<I>, sym: u32) -> u32
where
    I: Allocator,
{
    v_addr_to_addr(elf, sym).unwrap_or(0)
}
