use plank_core::chunked_arena::ChunkedArena;
use std::{
    cell::{Cell, UnsafeCell},
    fmt,
    mem::align_of,
    num::NonZero,
};

use hashbrown::{DefaultHashBuilder, HashSet, HashTable, hash_table::Entry};
use plank_session::{Session, SourceSpan, SrcLoc, StrId};

use crate::ValueId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Field {
    pub name: StrId,
    pub ty: TypeId,
    pub def_span: SourceSpan,
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
    let () = _HEADER_FIELD_ALIGN_EQ;
    align_of::<StructHeader>()
};

#[derive(Debug, Clone, Copy)]
pub struct StructView<'a> {
    pub def_loc: SrcLoc,
    pub type_index: ValueId,
    pub name: &'a Cell<Option<StrId>>,
    pub fields: &'a [Field],
}

impl StructView<'_> {
    fn as_info(&self) -> StructInfo<'_> {
        StructInfo { def_loc: self.def_loc, type_index: self.type_index, fields: self.fields }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructInfo<'a> {
    pub type_index: ValueId,
    pub def_loc: SrcLoc,
    pub fields: &'a [Field],
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(test, derive(enum_iterator::Sequence))]
pub enum PrimitiveType {
    Void,
    U256,
    Bool,
    MemoryPointer,
    Type,
    Function,
    Never,
}

impl PrimitiveType {
    pub const fn name(self) -> &'static str {
        use plank_session::builtins::builtin_names;
        match self {
            PrimitiveType::Void => builtin_names::VOID,
            PrimitiveType::U256 => builtin_names::U256,
            PrimitiveType::Bool => builtin_names::BOOL,
            PrimitiveType::MemoryPointer => builtin_names::MEMORY_POINTER,
            PrimitiveType::Type => builtin_names::TYPE,
            PrimitiveType::Function => builtin_names::FUNCTION,
            PrimitiveType::Never => builtin_names::NEVER,
        }
    }

    pub const fn comptime_only(self) -> bool {
        match self {
            PrimitiveType::Void
            | PrimitiveType::U256
            | PrimitiveType::Bool
            | PrimitiveType::MemoryPointer
            | PrimitiveType::Never => false,
            PrimitiveType::Type | PrimitiveType::Function => true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Type<'fields> {
    Primitive(PrimitiveType),
    Struct(StructView<'fields>),
}

pub struct TypeInterner {
    comptime_only: UnsafeCell<HashSet<StructRef>>,
    dedup: UnsafeCell<HashTable<StructRef>>,
    arena: ChunkedArena<MIN_STRUCT_FIELD_ALIGN>,
    hasher: DefaultHashBuilder,
}

impl Default for TypeInterner {
    fn default() -> Self {
        Self::new()
    }
}

/// ID that uniquely identifies every Plank type. Should only be created by the `TypeInterner` or
/// the primitive type constants.
///
/// # Representation
/// For structs the [`ChunkedArena`] offset is stored verbatim. Thanks to the guarantees from
/// [`alloc_append`](ChunkedArena::alloc_append) we know that offsets will be a multiple of our
/// chosen alignment ([`MIN_STRUCT_FIELD_ALIGN`]). This lets us uniquely identify primitive types
/// by ensuring they are *not* multiples of [`MIN_STRUCT_FIELD_ALIGN`], this is done by setting the
/// lower bit via [`IS_PRIMITIVE_FLAG`](TypeId::IS_PRIMITIVE_FLAG).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub(crate) NonZero<u32>);

impl std::fmt::Debug for TypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            TypeId::VOID => write!(f, "TypeId::VOID"),
            TypeId::U256 => write!(f, "TypeId::U256"),
            TypeId::BOOL => write!(f, "TypeId::BOOL"),
            TypeId::MEMORY_POINTER => write!(f, "TypeId::MEMORY_POINTER"),
            TypeId::TYPE => write!(f, "TypeId::TYPE"),
            TypeId::FUNCTION => write!(f, "TypeId::FUNCTION"),
            TypeId::NEVER => write!(f, "TypeId::NEVER"),
            other => write!(f, "TypeId({})", other.get()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructRef(u32);

impl TypeId {
    pub const VOID: TypeId = TypeId::from_primitive(PrimitiveType::Void);
    pub const U256: TypeId = TypeId::from_primitive(PrimitiveType::U256);
    pub const BOOL: TypeId = TypeId::from_primitive(PrimitiveType::Bool);
    pub const MEMORY_POINTER: TypeId = TypeId::from_primitive(PrimitiveType::MemoryPointer);
    pub const TYPE: TypeId = TypeId::from_primitive(PrimitiveType::Type);
    pub const FUNCTION: TypeId = TypeId::from_primitive(PrimitiveType::Function);
    pub const NEVER: TypeId = TypeId::from_primitive(PrimitiveType::Never);

    const IS_PRIMITIVE_FLAG: u32 = 1;

    pub(crate) const fn new(value: u32) -> Self {
        TypeId(unsafe {
            let inner = value.checked_add(1).expect("overflow");
            NonZero::new_unchecked(inner)
        })
    }

    pub const fn get(self) -> u32 {
        unsafe { self.0.get().unchecked_sub(1) }
    }

    pub const fn is_primitive(self) -> bool {
        (self.get() & Self::IS_PRIMITIVE_FLAG) != 0
    }

    pub const fn from_primitive(primitive: PrimitiveType) -> TypeId {
        const { assert!(Self::IS_PRIMITIVE_FLAG < MIN_STRUCT_FIELD_ALIGN as u32) };
        let pid = primitive as u32;
        TypeId::new((pid * MIN_STRUCT_FIELD_ALIGN as u32) | Self::IS_PRIMITIVE_FLAG)
    }

    pub const fn from_struct(offset: StructRef) -> TypeId {
        TypeId::new(offset.0)
    }

    pub const fn as_primitive(self) -> Result<PrimitiveType, StructRef> {
        match self {
            TypeId::VOID => Ok(PrimitiveType::Void),
            TypeId::U256 => Ok(PrimitiveType::U256),
            TypeId::BOOL => Ok(PrimitiveType::Bool),
            TypeId::MEMORY_POINTER => Ok(PrimitiveType::MemoryPointer),
            TypeId::TYPE => Ok(PrimitiveType::Type),
            TypeId::FUNCTION => Ok(PrimitiveType::Function),
            TypeId::NEVER => Ok(PrimitiveType::Never),
            ty => Err(StructRef(ty.get())),
        }
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
}

impl From<StructRef> for TypeId {
    fn from(value: StructRef) -> Self {
        Self::from_struct(value)
    }
}

impl TypeInterner {
    pub fn new() -> Self {
        Self {
            comptime_only: UnsafeCell::new(HashSet::new()),
            arena: ChunkedArena::new(),
            dedup: UnsafeCell::new(HashTable::new()),
            hasher: DefaultHashBuilder::default(),
        }
    }

    pub fn is_comptime_only(&self, ty: TypeId) -> bool {
        match ty.as_primitive() {
            Ok(prim) => prim.comptime_only(),
            Err(r#struct) => unsafe { (*self.comptime_only.get()).contains(&r#struct) },
        }
    }

    pub fn intern_struct(&self, info: StructInfo<'_>) -> StructRef {
        use std::hash::BuildHasher;
        let hash = self.hasher.hash_one(info);
        // Safety: We only retain the `&mut` reference for the duration of this function and
        // `lookup_struct` and `push_struct` don't reference `self.dedup` at all.
        let dedup = unsafe { &mut (*self.dedup.get()) };
        let entry = dedup.entry(
            hash,
            |&r#struct| self.lookup_struct(r#struct).as_info() == info,
            |&r#struct| self.hasher.hash_one(self.lookup_struct(r#struct).as_info()),
        );

        match entry {
            Entry::Occupied(occupied) => *occupied.get(),
            Entry::Vacant(vacant_entry) => {
                let new_ref = self.push_struct(info);
                vacant_entry.insert(new_ref);
                if info.fields.iter().any(|&field| self.is_comptime_only(field.ty)) {
                    unsafe { (*self.comptime_only.get()).insert(new_ref) };
                }
                new_ref
            }
        }
    }

    pub fn lookup<'s>(&'s self, ty: TypeId) -> Type<'s> {
        match ty.as_primitive() {
            Ok(prim) => Type::Primitive(prim),
            Err(r#struct) => Type::Struct(self.lookup_struct(r#struct)),
        }
    }

    pub fn lookup_struct<'s>(&'s self, r#struct: StructRef) -> StructView<'s> {
        unsafe {
            let header_ptr = self.arena.get(r#struct.0) as *const StructHeader;
            let header = &(*header_ptr);
            let fields_start = header_ptr.add(1) as *const Field;

            StructView {
                def_loc: header.def_loc,
                type_index: header.type_index,
                name: &header.name,
                fields: core::slice::from_raw_parts(fields_start, header.total_fields as usize),
            }
        }
    }

    pub fn fmt_struct(
        &self,
        f: &mut impl fmt::Write,
        r#struct: StructRef,
        session: &Session,
    ) -> fmt::Result {
        let view = self.lookup_struct(r#struct);
        if let Some(name) = view.name.get() {
            return f.write_str(session.lookup_name(name));
        }
        let (line, col) = session.offset_to_line_col(view.def_loc.source, view.def_loc.span.start);
        let source = &session.get_source(view.def_loc.source);
        write!(f, "struct#{}@{}:{line}:{col}", r#struct.0, source.path.to_str().unwrap())
    }

    pub fn format<'a>(&'a self, sess: &'a Session, ty: TypeId) -> FmtType<'a> {
        FmtType { types: self, sess, ty }
    }

    fn push_struct<'s, 'a>(&'s self, r#struct: StructInfo<'a>) -> StructRef {
        let required_space =
            std::mem::size_of::<StructHeader>() + std::mem::size_of_val(r#struct.fields);

        unsafe {
            // The `_HEADER_FIELD_ALIGN_EQ` const assert is what tells us that it's safe to cast to
            // field & struct header pointers.
            let () = _HEADER_FIELD_ALIGN_EQ;
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

            debug_assert!(offset.is_multiple_of(MIN_STRUCT_FIELD_ALIGN as u32));
            StructRef(offset)
        }
    }
}

pub struct FmtType<'a> {
    types: &'a TypeInterner,
    sess: &'a Session,
    ty: TypeId,
}

impl std::fmt::Display for FmtType<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.ty.as_primitive() {
            Ok(prim) => write!(f, "{}", prim.name()),
            Err(r#struct) => self.types.fmt_struct(f, r#struct, self.sess),
        }
    }
}

impl fmt::Debug for TypeInterner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TypeInterner {{ <opaque> }}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plank_session::{SourceId, SrcLoc, StrId, ZERO_SPAN};

    fn dummy_src_loc(id: u32) -> SrcLoc {
        SrcLoc::new(SourceId::new(id), ZERO_SPAN)
    }

    fn dummy_struct_info(fields: &[Field]) -> StructInfo<'_> {
        StructInfo { type_index: ValueId::VOID, def_loc: dummy_src_loc(0), fields }
    }

    #[test]
    fn primitive_types_have_unique_ids() {
        use std::collections::HashSet;
        let ids: HashSet<TypeId> =
            enum_iterator::all::<PrimitiveType>().map(TypeId::from_primitive).collect();
        assert_eq!(ids.len(), enum_iterator::all::<PrimitiveType>().count());
    }

    #[test]
    fn struct_intern_deduplication() {
        let interner = TypeInterner::new();
        let fields = [Field { name: StrId::new(0), ty: TypeId::U256, def_span: ZERO_SPAN }];

        let a = interner.intern_struct(dummy_struct_info(&fields));
        let b = interner.intern_struct(dummy_struct_info(&fields));
        assert_eq!(a, b);

        let different = [Field { name: StrId::new(1), ty: TypeId::BOOL, def_span: ZERO_SPAN }];
        let c = interner.intern_struct(dummy_struct_info(&different));
        assert_ne!(a, c);
    }

    #[test]
    fn struct_refs_are_aligned() {
        let interner = TypeInterner::new();
        let f = Field { name: StrId::new(0), ty: TypeId::U256, def_span: ZERO_SPAN };

        let a = interner.intern_struct(dummy_struct_info(&[f]));
        let b = interner.intern_struct(dummy_struct_info(&[f, f]));
        let c = interner.intern_struct(dummy_struct_info(&[f, f, f]));

        for r#struct in [a, b, c] {
            let raw = TypeId::from_struct(r#struct).get();
            assert!(raw.is_multiple_of(MIN_STRUCT_FIELD_ALIGN as u32));
        }
    }

    #[test]
    fn struct_different_src_loc_interns_separately() {
        let interner = TypeInterner::new();
        let fields = [Field { name: StrId::new(0), ty: TypeId::U256, def_span: ZERO_SPAN }];

        let a_info =
            StructInfo { type_index: ValueId::VOID, def_loc: dummy_src_loc(0), fields: &fields };
        let b_info =
            StructInfo { type_index: ValueId::VOID, def_loc: dummy_src_loc(1), fields: &fields };

        let a = interner.intern_struct(a_info);
        let b = interner.intern_struct(b_info);
        assert_ne!(a, b);
    }

    #[test]
    fn is_comptime_only_nested_struct() {
        let interner = TypeInterner::new();

        let inner_fields = [Field { name: StrId::new(0), ty: TypeId::TYPE, def_span: ZERO_SPAN }];
        let inner = interner.intern_struct(dummy_struct_info(&inner_fields));
        let inner_ty = TypeId::from_struct(inner);
        assert!(interner.is_comptime_only(inner_ty));

        let outer_fields = [Field { name: StrId::new(1), ty: inner_ty, def_span: ZERO_SPAN }];
        let outer = interner.intern_struct(dummy_struct_info(&outer_fields));
        let outer_ty = TypeId::from_struct(outer);
        assert!(interner.is_comptime_only(outer_ty));

        let runtime_fields = [Field { name: StrId::new(2), ty: TypeId::U256, def_span: ZERO_SPAN }];
        let runtime = interner.intern_struct(dummy_struct_info(&runtime_fields));
        assert!(!interner.is_comptime_only(TypeId::from_struct(runtime)));
    }
}
