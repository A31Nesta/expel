#[derive(Default)]
pub struct ElfSection {
    pub v_addr: u32, // originally `uintptr_t`
    pub offset: u32, // originally `off_t`
    pub addr: u32,   // originally `uintptr_t`
    pub size: u32,   // originally `size_t`
}

impl ElfSection {
    pub fn new(v_addr: u32, offset: u32, addr: u32, size: u32) -> Self {
        Self {
            v_addr,
            offset,
            addr,
            size,
        }
    }
}

#[derive(Default)]
pub struct ElfSections {
    pub text: ElfSection,
    pub data: ElfSection,
    pub rodata: ElfSection,
    pub data_rel_ro: ElfSection,
    pub bss: ElfSection,
}
