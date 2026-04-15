use plank_core::chunked_arena::ChunkedArena;
use std::{
    cell::{Cell, UnsafeCell},
    mem::align_of,
    num::NonZero,
};

use hashbrown::HashTable;
use plank_session::{SrcLoc, StrId};

use crate::ValueId;

const MAX_CHUNKS: usize = 22;
const FIRST_CHUNK_SIZE_BYTES: usize = 1024;

const _MAX_BYTES_FITS_IN_U32: () =
    assert!(FIRST_CHUNK_SIZE_BYTES * 2usize.pow(MAX_CHUNKS as u32) == 2usize.pow(u32::BITS));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Field {
    pub name: StrId,
    pub ty: TypeId,
}

struct StructHeader {
    def_loc: SrcLoc,
    type_index: ValueId,
    name: Cell<Option<StrId>>,
    total_fields: u32,
}

const _HEADER_FIELD_ALIGN_EQ: () =
    const { assert!(align_of::<Field>() == align_of::<StructHeader>()) };

const MIN_STRUCT_FIELD_ALIGN: usize = {
    _ = _HEADER_FIELD_ALIGN_EQ;
    align_of::<StructHeader>()
};

#[derive(Debug, Clone, Copy)]
pub struct StructView<'a> {
    pub def_loc: SrcLoc,
    pub type_index: ValueId,
    pub name: &'a Cell<Option<StrId>>,
    pub fields: &'a [Field],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructInfo<'a> {
    pub def_loc: SrcLoc,
    pub type_index: ValueId,
    pub fields: &'a [Field],
}

#[derive(Debug, Clone)]
pub enum Type<'fields> {
    Void,
    Int,
    Bool,
    MemoryPointer,
    Type,
    Function,
    Never,
    Struct(StructView<'fields>),
}

pub struct TypeInterner {
    dedup: UnsafeCell<HashTable<u32>>,
    arena: ChunkedArena<MIN_STRUCT_FIELD_ALIGN>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub(crate) NonZero<u32>);

impl TypeId {
    pub(crate) const fn new(value: u32) -> Self {
        TypeId(unsafe {
            let inner = value.checked_add(1).expect("overflow");
            NonZero::new_unchecked(inner)
        })
    }

    pub const fn const_get(self) -> u32 {
        self.0.get() - 1
    }

    pub fn get(self) -> u32 {
        self.const_get()
    }

    pub const VOID: TypeId = TypeId::new(0);
    pub const U256: TypeId = TypeId::new(1);
    pub const BOOL: TypeId = TypeId::new(2);
    pub const MEMORY_POINTER: TypeId = TypeId::new(3);
    pub const TYPE: TypeId = TypeId::new(4);
    pub const FUNCTION: TypeId = TypeId::new(5);
    pub const NEVER: TypeId = TypeId::new(6);

    pub const LAST_FIXED_ID: TypeId = Self::NEVER;
    pub const STRUCT_IDS_OFFSET: u32 = Self::LAST_FIXED_ID.const_get() + 1;

    pub const fn is_struct(self) -> bool {
        self.const_get() > Self::LAST_FIXED_ID.const_get()
    }

    pub fn is_assignable_to(self, target: TypeId) -> bool {
        self == target || self == TypeId::NEVER
    }

    pub fn unify(&mut self, other: TypeId) -> Result<(), TypeId> {
        if *self == TypeId::NEVER {
            *self = other;
            return Ok(());
        }
        if other == TypeId::NEVER || *self == other {
            return Ok(());
        }
        Err(*self)
    }

    pub fn primitive_name(self) -> Option<&'static str> {
        use plank_session::builtins::builtin_names;
        Some(match self {
            TypeId::VOID => builtin_names::VOID,
            TypeId::U256 => builtin_names::U256,
            TypeId::BOOL => builtin_names::BOOL,
            TypeId::MEMORY_POINTER => builtin_names::MEMORY_POINTER,
            TypeId::TYPE => builtin_names::TYPE,
            TypeId::FUNCTION => builtin_names::FUNCTION,
            TypeId::NEVER => builtin_names::NEVER,
            _ => return None,
        })
    }
}

impl TypeInterner {
    pub fn new() -> Self {
        Self { arena: ChunkedArena::new(), dedup: UnsafeCell::new(HashTable::new()) }
    }

    fn push_struct<'s, 'a>(&'s self, r#struct: StructInfo<'a>) -> (u32, StructView<'s>) {
        let required_space = std::mem::size_of::<StructHeader>()
            + std::mem::size_of::<Field>() * r#struct.fields.len();

        unsafe {
            // The `_HEADER_FIELD_ALIGN_EQ` const assert is what tells us that it's safe to cast to
            // field & struct header pointers.
            _ = _HEADER_FIELD_ALIGN_EQ;
            let (offset, new_struct_ptr) = self.arena.alloc_append(required_space);

            let fields_start = new_struct_ptr.byte_add(size_of::<StructHeader>()) as *mut Field;
            let mut field_ptr = fields_start;
            for &field in r#struct.fields {
                field_ptr.write(field);
                field_ptr = field_ptr.add(1);
            }

            let header_ptr = new_struct_ptr as *mut StructHeader;
            header_ptr.write(StructHeader {
                def_loc: r#struct.def_loc,
                type_index: r#struct.type_index,
                name: Cell::new(None),
                total_fields: r#struct.fields.len() as u32,
            });

            let interned = StructView {
                def_loc: r#struct.def_loc,
                type_index: r#struct.type_index,
                name: &(&*header_ptr).name,
                fields: core::slice::from_raw_parts(fields_start, r#struct.fields.len()),
            };

            (offset, interned)
        }
    }
}
