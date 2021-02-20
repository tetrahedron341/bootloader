use bit_field::BitField;
use bitflags::bitflags;

#[derive(Debug, Clone)]
pub struct GlobalDescriptorTable {
    pub table: [u64; 8],
    next_free: usize,
}

impl GlobalDescriptorTable {
    /// Creates an empty GDT.
    #[inline]
    pub const fn new() -> GlobalDescriptorTable {
        GlobalDescriptorTable {
            table: [0; 8],
            next_free: 1,
        }
    }

    /// Adds the given segment descriptor to the GDT, returning the segment selector.
    ///
    /// Panics if the GDT has no free entries left.
    #[inline]
    pub fn add_entry(&mut self, entry: Descriptor) -> u16 {
        let index = self.push(entry.0);

        index as u16
    }

    /// Loads the GDT in the CPU using the `lgdt` instruction. This does **not** alter any of the
    /// segment registers; you **must** (re)load them yourself.
    #[inline]
    pub unsafe fn load(&self) {
        use core::mem::size_of;

        /// A struct describing a pointer to a descriptor table (GDT / IDT).
        /// This is in a format suitable for giving to 'lgdt' or 'lidt'.
        #[derive(Debug, Clone, Copy)]
        #[repr(C, packed)]
        struct DescriptorTablePointer {
            /// Size of the DT.
            pub limit: u16,
            /// Pointer to the memory region containing the DT.
            pub base: u32,
        }

        let ptr = DescriptorTablePointer {
            base: self.table.as_ptr() as u32,
            limit: (self.table.len() * size_of::<u64>() - 1) as u16,
        };

        use crate::println;
        println!("GDT -");
        println!("    {:#08x}", self.table[0]);
        println!("    {:#08x}", self.table[1]);
        println!("    {:#08x}", self.table[2]);
        println!("    {:#08x}", self.table[3]);
        println!("    {:#08x}", self.table[4]);
        println!("    {:#08x}", self.table[5]);
        println!("    {:#08x}", self.table[6]);
        println!("    {:#08x}", self.table[7]);

        asm!("lgdt [{}]",
             in(reg) &ptr,
             options(nostack)
        );
    }

    #[inline]
    fn push(&mut self, value: u64) -> usize {
        if self.next_free < self.table.len() {
            let index = self.next_free;
            self.table[index] = value;
            self.next_free += 1;

            index
        } else {
            panic!("GDT full");
        }
    }
}

#[derive(Debug, Clone)]
pub struct Descriptor(u64);

bitflags! {
    /// Flags for a GDT descriptor. Not all flags are valid for all descriptor types.
    pub struct DescriptorFlags: u64 {
        /// The CPU sets this value to one when the segment is accessed
        const ACCESSED          = 1 << 40;
        /// For data segments, this flag sets the segment as writable. For code
        /// segments, it defines whether the segment is readable.
        const READABLE_WRITABLE = 1 << 41;
        /// Marks a code segment as “conforming”. This influences the privilege checks that
        /// occur on control transfers.
        const CONFORMING        = 1 << 42;
        /// This flag must be set for code segments.
        const EXECUTABLE        = 1 << 43;
        /// This flag must be set for user segments (in contrast to system segments).
        const USER_SEGMENT      = 1 << 44;
        /// Must be set for any segment, causes a segment not present exception if not set.
        const PRESENT           = 1 << 47;
        /// Must be set for long mode code segments.
        const LONG_MODE         = 1 << 53;

        /// The DPL for this descriptor is Ring 3
        const DPL_RING_3        = 3 << 45;

        /// Is this segment available for use
        const AVAILABLE         = 1 << 52;

        /// If set, this page is a 32 bit descriptor
        const SIZE              = 1 << 54;

        /// If set, limit is in 4k pages
        const GRANULARITY       = 1 << 55;
    }
}

impl Descriptor {
    /// Creates a null descriptor
    #[inline]
    pub fn null_descriptor() -> Descriptor {
        Descriptor(0)
    }

    /// Creates a segment descriptor for a protected mode kernel code segment.
    #[inline]
    pub fn kernel_code_segment() -> Descriptor {
        use self::DescriptorFlags as Flags;

        let flags =
            Flags::USER_SEGMENT | Flags::PRESENT | Flags::READABLE_WRITABLE | Flags::ACCESSED | Flags::SIZE | Flags::EXECUTABLE;

        Descriptor(flags.bits()).with_flat_limit()
    }

    /// Creates a segment descriptor for a protected mode kernel data segment.
    #[inline]
    pub fn kernel_data_segment() -> Descriptor {
        use self::DescriptorFlags as Flags;

        let flags =
            Flags::USER_SEGMENT | Flags::PRESENT | Flags::READABLE_WRITABLE | Flags::ACCESSED | Flags::SIZE;
        Descriptor(flags.bits()).with_flat_limit()
    }

    /// Creates a segment descriptor for a protected mode ring 3 data segment.
    #[inline]
    pub fn user_data_segment() -> Descriptor {
        use self::DescriptorFlags as Flags;

        let flags =
            Flags::USER_SEGMENT | Flags::PRESENT | Flags::READABLE_WRITABLE | Flags::ACCESSED | Flags::DPL_RING_3;

        Descriptor(flags.bits()).with_flat_limit()
    }

    /// Creates a segment descriptor for a protected mode ring 3 code segment.
    #[inline]
    pub fn user_code_segment() -> Descriptor {
        use self::DescriptorFlags as Flags;

        let flags =
            Flags::USER_SEGMENT | Flags::PRESENT | Flags::READABLE_WRITABLE | Flags::ACCESSED | Flags::EXECUTABLE | Flags::DPL_RING_3;

        Descriptor(flags.bits()).with_flat_limit()
    }

    /// Creates a TSS system descriptor for the given TSS.
    #[inline]
    pub fn tss_segment(tss: &TaskStateSegment) -> Descriptor {
        use self::DescriptorFlags as Flags;
        use core::mem::size_of;

        let ptr = tss as *const _ as u64;


        let mut val: u64 = (Flags::PRESENT | Flags::EXECUTABLE | Flags::ACCESSED | Flags::SIZE | Flags::DPL_RING_3).bits();

        // base
        val.set_bits(16..40, ptr.get_bits(0..24));
        val.set_bits(56..64, ptr.get_bits(24..32));

        // limit (the `-1` in needed since the bound is inclusive)
        val.set_bits(0..16, ((size_of::<TaskStateSegment>() - 1) as u64).get_bits(0..16));

        Descriptor(val)
    }

    fn with_flat_limit(mut self) -> Self {
        // limit_low
        self.0.set_bits(0..16, 0xffff);

        // limit high
        // self.0.set_bits(48..52, 0xff);
        self.0.set_bit(48, true);
        self.0.set_bit(49, true);
        self.0.set_bit(50, true);
        self.0.set_bit(51, true);

        // granularity
        self.0 |= DescriptorFlags::GRANULARITY.bits();

        self
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct TaskStateSegment {
    /// Used for hardware task switching
    prev_tss: u32,
    /// The full 64-bit canonical forms of the stack pointers (RSP) for privilege levels 0-2.
    pub privilege_stack_table: [Stack; 3],

    cr3: u32,
    eip: u32,
    eflags: u32,
    eax: u32,
    ecx: u32,
    edx: u32,
    ebx: u32,
    esp: u32,
    ebp: u32,
    esi: u32,
    edi: u32,
    es: u32,
    cs: u32,
    ss: u32,
    ds: u32,
    fs: u32,
    gs: u32,
    ldt: u32,
    trap: u16,
    pub iomap_base: u16,
}

impl TaskStateSegment {
    /// Creates a new TSS with zeroed privilege and interrupt stack table and a zero
    /// `iomap_base`.
    #[inline]
    pub const fn new() -> TaskStateSegment {
        TaskStateSegment {
            privilege_stack_table: [Stack::zero(); 3],
            iomap_base: 0,
            prev_tss: 0,
            cr3: 0,
            eip: 0,
            eflags: 0,
            eax: 0,
            ecx: 0,
            edx: 0,
            ebx: 0,
            esp: 0,
            ebp: 0,
            esi: 0,
            edi: 0,
            es: 0,
            cs: 0,
            ss: 0,
            ds: 0,
            fs: 0,
            gs: 0,
            ldt: 0,
            trap: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct Stack {
    pub esp: u32,
    pub ss: u32,
}

impl Stack {
    const fn zero() -> Self {
        Stack { esp: 0, ss: 0 }
    }
}
