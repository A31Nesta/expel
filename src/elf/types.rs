use core::ffi::{c_char, c_int};

use allocator_api2::alloc::Allocator;
#[cfg(feature = "bus-address-mirror")]
use allocator_api2::vec::Vec;
#[cfg(feature = "dlso")]
use allocator_api2::vec::Vec;

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

pub type ElfMain = extern "C" fn(argc: c_int, argv: *mut *mut c_char) -> c_int;

/// ELF struct that will be used throughout this crate, adapted from
/// the ELF Loader IDF Component by Espressif.
///
/// Comments and names also adapted from the `esp_elf_t` struct in the
/// original loader component
///
/// > https://github.com/espressif/esp-iot-solution/blob/ef13dcfd5aa18c4d6dca3d89d18173f8ae180a5f/components/elf_loader/include/private/elf_types.h#L264
pub struct Elf<I, D>
where
    I: Allocator, // Instructions Allocator
    D: Allocator, // Data Allocator
{
    /// Instruction buffer pointer
    #[cfg(feature = "bus-address-mirror")]
    pub ptext: Vec<u32, I>,
    /// Data buffer pointer
    #[cfg(feature = "bus-address-mirror")]
    pub pdata: Vec<u8, D>,

    /// Segment buffer pointer
    #[cfg(not(feature = "bus-address-mirror"))]
    pub psegment: *mut u8,
    /// Start virtual address of segment
    #[cfg(not(feature = "bus-address-mirror"))]
    pub svaddr: u32,

    /// "`.bss`", "`.data`", "`.rodata`", "`.text`"
    pub sections: ElfSections,
    // Entry pointer of ELF
    pub entry: Option<*mut ElfMain>,

    /// `.text` symbol offset
    #[cfg(feature = "set-mmu")]
    pub text_off: u32,
    /// MMU unit offset
    #[cfg(feature = "set-mmu")]
    pub mmu_off: u32,
    /// MMU unit total number
    #[cfg(feature = "set-mmu")]
    pub mmu_num: u32,

    /// Number of symbols in the dynamic object
    #[cfg(feature = "dlso")]
    pub num: u16,
    /// Symbol table of dynamic object pointer
    #[cfg(feature = "dlso")]
    pub symtab:
        Vec<compile_error!("DLSO feature | ELF Struct: `esp_symtab_t` not implemented yet")>,
}

impl<I, D> Elf<I, D>
where
    I: Allocator,
    D: Allocator,
{
    pub fn new(
        #[cfg(feature = "bus-address-mirror")] ptext: Vec<u32, I>,
        #[cfg(feature = "bus-address-mirror")] pdata: Vec<u8, D>,

        #[cfg(not(feature = "bus-address-mirror"))] psegment: *mut u8,
        #[cfg(not(feature = "bus-address-mirror"))] svaddr: u32,

        sections: ElfSections,
        entry: *mut ElfMain,

        #[cfg(feature = "set-mmu")] text_off: u32,
        #[cfg(feature = "set-mmu")] mmu_off: u32,
        #[cfg(feature = "set-mmu")] mmu_num: u32,

        #[cfg(feature = "dlso")] num: u16,
        #[cfg(feature = "dlso")] symtab: Vec<
            compile_error!("DLSO feature | ELF Struct: `esp_symtab_t` not implemented yet"),
        >,
    ) -> Self {
        Self {
            #[cfg(feature = "bus-address-mirror")]
            ptext,
            #[cfg(feature = "bus-address-mirror")]
            pdata,

            #[cfg(not(feature = "bus-address-mirror"))]
            psegment,
            #[cfg(not(feature = "bus-address-mirror"))]
            svaddr,

            sections,
            entry: Some(entry),

            #[cfg(feature = "set-mmu")]
            text_off,
            #[cfg(feature = "set-mmu")]
            mmu_off,
            #[cfg(feature = "set-mmu")]
            mmu_num,

            #[cfg(feature = "dlso")]
            num,
            #[cfg(feature = "dlso")]
            symtab,
        }
    }

    pub fn empty(iram_alloc: I, data_alloc: D) -> Self {
        Self {
            #[cfg(feature = "bus-address-mirror")]
            ptext: Vec::new_in(iram_alloc),
            #[cfg(feature = "bus-address-mirror")]
            pdata: Vec::new_in(data_alloc),
            #[cfg(not(feature = "bus-address-mirror"))]
            psegment: Default::default(),
            #[cfg(not(feature = "bus-address-mirror"))]
            svaddr: Default::default(),
            sections: Default::default(),
            entry: Default::default(),
            #[cfg(feature = "set-mmu")]
            text_off: Default::default(),
            #[cfg(feature = "set-mmu")]
            mmu_off: Default::default(),
            #[cfg(feature = "set-mmu")]
            mmu_num: Default::default(),
            #[cfg(feature = "dlso")]
            num: Default::default(),
            #[cfg(feature = "dlso")]
            symtab: Default::default(),
        }
    }
}
