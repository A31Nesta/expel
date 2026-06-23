use core::{
    cell::UnsafeCell,
    mem::transmute,
    sync::atomic::{AtomicPtr, Ordering},
};

pub type SymbolResolver = fn(sym_name: &str) -> usize;

pub static CURRENT_RESOLVER: AtomicPtr<SymbolResolver> =
    AtomicPtr::new(elf_find_sym_default as *mut _);

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

pub fn elf_find_symbol(sym_name: &str) -> usize {
    let resolver: SymbolResolver = unsafe { transmute(CURRENT_RESOLVER.load(Ordering::SeqCst)) };
    resolver(sym_name)
}

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
/// TODO: Create a proper symbol table lookup implementation
fn elf_find_sym_default(sym_name: &str) -> usize {
    for sym in unsafe { *SYMBOLS.symbols.get() } {
        if sym.name == sym_name {
            return sym.addr;
        }
    }

    0
}
