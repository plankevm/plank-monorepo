bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct TypeFlags: u8 {
        const NONE                = 0;
        const RUNTIME_ONLY        = 1 << 0;
        const COMPTIME_ONLY       = 1 << 1;
        const UNINIT_INCOMPATIBLE = 1 << 2;

        const UNINITIALIZABLE_MIXED = TypeFlags::RUNTIME_ONLY.bits() | TypeFlags::COMPTIME_ONLY.bits();
    }
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(enum_iterator::Sequence))]
pub enum PrimitiveType {
    U256,
    Bool,
    MemoryPointer,
    Type,
    Function,
    CBytes,
    Never,
}

impl PrimitiveType {
    pub const fn name(self) -> &'static str {
        use plank_session::builtins::builtin_names;
        match self {
            PrimitiveType::U256 => builtin_names::U256,
            PrimitiveType::Bool => builtin_names::BOOL,
            PrimitiveType::MemoryPointer => builtin_names::MEMORY_POINTER,
            PrimitiveType::Type => builtin_names::TYPE,
            PrimitiveType::Function => builtin_names::FUNCTION,
            PrimitiveType::CBytes => builtin_names::CBYTES,
            PrimitiveType::Never => builtin_names::NEVER,
        }
    }

    pub const fn flags(self) -> TypeFlags {
        match self {
            PrimitiveType::U256 | PrimitiveType::Bool => TypeFlags::NONE,
            PrimitiveType::MemoryPointer => TypeFlags::RUNTIME_ONLY,
            PrimitiveType::Type => TypeFlags::COMPTIME_ONLY,
            PrimitiveType::Function => TypeFlags::from_bits_retain(
                TypeFlags::COMPTIME_ONLY.bits() | TypeFlags::UNINIT_INCOMPATIBLE.bits(),
            ),
            PrimitiveType::CBytes => TypeFlags::COMPTIME_ONLY,
            PrimitiveType::Never => TypeFlags::UNINIT_INCOMPATIBLE,
        }
    }

    pub const fn comptime_only(self) -> bool {
        self.flags().contains(TypeFlags::COMPTIME_ONLY)
    }
}
