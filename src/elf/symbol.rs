use core::cell::UnsafeCell;

use allocator_api2::alloc::Allocator;

#[cfg(feature = "logging")]
use defmt::info;

use crate::elf::types::Elf;

#[derive(Clone, Copy)]
pub struct Symbol {
    name: &'static str,
    addr: usize,
}

struct SymbolTable {
    symbols: UnsafeCell<[Symbol; MAX_SYMBOLS]>,
}
unsafe impl Sync for SymbolTable {}

const MAX_SYMBOLS: usize = 128;
static SYMBOLS: SymbolTable = SymbolTable {
    symbols: UnsafeCell::new([Symbol { name: "", addr: 0 }; MAX_SYMBOLS]),
};

/// Registers a symbol and makes it available to loaded programs
pub fn elf_register_symbol(sym_name: &'static str, address: usize) {
    // Check: is it already there?
    // We do it in a single loop like in Espressif's version, which can fail if we
    // unregister symbols as well but I won't be doing that.
    unsafe {
        let symbols = &mut *SYMBOLS.symbols.get();
        for sym in symbols.iter_mut() {
            if sym.name == sym_name {
                // Symbol was already registered
                return;
            } else if sym.addr == 0 {
                sym.name = sym_name;
                sym.addr = address;
                return;
            }
        }
    }

    panic!("This symbol doesn't fit in the table! :((((");
}

pub fn elf_unregister_symbol(sym_name: &str) {
    unsafe {
        let symbols = &mut *SYMBOLS.symbols.get();
        for sym in symbols.iter_mut() {
            if sym.name == sym_name {
                sym.name = "";
                sym.addr = 0;
                return;
            }
        }
    }
}

/// Find symbol address by name
pub fn elf_find_symbol<T>(
    sym_name: &str,
    elf_opt: Option<&Elf<T>>,
    symbol_opt: Option<&elf::symbol::Symbol>,
) -> usize
where
    T: Allocator,
{
    for sym in unsafe { *SYMBOLS.symbols.get() } {
        if sym.name == sym_name && !sym.name.is_empty() {
            #[cfg(feature = "logging")]
            info!("Symbol `{}` found; addr: `{}`", sym_name, sym.addr);
            return sym.addr;
        }
    }

    // info!("Symbol not found; attempting to obtain without name...");

    // Adapted from this code:
    // https://github.com/niicoooo/esp32-elfloader/blob/52531c631f1c723e6931966170f0b20ef0efa6db/components/elfloader/loader.c#L313
    // Find without name. We check like this because we can't `if let` for 2 thingies
    if elf_opt.is_none() || symbol_opt.is_none() {
        return 0;
    }
    let elf = elf_opt.unwrap();
    let symbol = symbol_opt.unwrap();

    #[cfg(feature = "logging")]
    info!(
        "sym={} shndx={} st_value={:#x}",
        sym_name, symbol.st_shndx, symbol.st_value
    );

    let section_opt = elf.sections.by_index(symbol.st_shndx as u32);

    if let Some(section) = section_opt {
        return section.addr as usize + symbol.st_value as usize;
    }

    // info!("Found nuffin'. Crash incoming, just to let you know");

    0
}
