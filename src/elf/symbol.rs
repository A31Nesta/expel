use core::{
    ffi::c_void,
    sync::atomic::{AtomicPtr, Ordering},
};

pub type SymbolResolver = fn(sym_name: &str) -> usize;

pub static CURRENT_RESOLVER: AtomicPtr<SymbolResolver> =
    AtomicPtr::new(elf_find_sym_default as *mut _);

pub struct Symbol {
    name: &'static str,
    sym: *mut c_void, // would this even work in Rust? lmao
}

pub fn elf_find_symbol(sym_name: &str) -> usize {
    let resolver = unsafe { *CURRENT_RESOLVER.load(Ordering::SeqCst) };
    resolver(sym_name)
}

/// Find symbol address by name
/// TODO: Create a proper symbol table lookup implementation
fn elf_find_sym_default(sym_name: &str) -> usize {
    // imagine this is our symbol table
    let syms = [Symbol {
        name: "malloc",
        sym: 0 as *mut c_void,
    }];

    for sym in syms {
        if sym.name == sym_name {
            return sym.sym.addr();
        }
    }

    0
}
