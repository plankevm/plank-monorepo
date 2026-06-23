use plank_core::{chunked_arena::ChunkedArena, list_of_lists::ListOfLists, newtype_index};
use std::{
    cell::{Cell, UnsafeCell},
    fmt,
    mem::{align_of, size_of},
    num::NonZero,
};

use crate::{
    ValueId, ValueInterner,
    primitive_types::{PrimitiveType, TypeFlags},
};
use hashbrown::{DefaultHashBuilder, HashTable, hash_table::Entry};
use plank_session::{Session, SourceSpan, SrcLoc, StrId};

newtype_index! {
    pub struct TypeNameArgsId;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeName {
    Plain(StrId),
    Parameterized { name: StrId, args: TypeNameArgsId },
}

const fn const_max(lhs: usize, rhs: usize) -> usize {
    if lhs > rhs { lhs } else { rhs }
}

const MIN_COMPOUND_ALIGN: usize = const_max(
    const_max(align_of::<StructHeader>(), align_of::<Field>()),
    const_max(align_of::<TupleHeader>(), align_of::<TypeId>()),
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Field {
    pub name: StrId,
    pub ty: TypeId,
    pub def_span: SourceSpan,
}

struct StructHeader {
    def_loc: SrcLoc,
    flags: TypeFlags,
    type_index: ValueId,
    name: Cell<Option<TypeName>>,
    total_fields: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct StructView<'a> {
    pub def_loc: SrcLoc,
    pub flags: TypeFlags,
    pub type_index: ValueId,
    pub name: &'a Cell<Option<TypeName>>,
    pub fields: &'a [Field],
}

impl<'a> StructView<'a> {
    fn as_key(self) -> StructKey<'a> {
        StructKey { def_loc: self.def_loc, type_index: self.type_index, fields: self.fields }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructKey<'a> {
    pub type_index: ValueId,
    pub def_loc: SrcLoc,
    pub fields: &'a [Field],
}

#[derive(Debug, Clone, Copy)]
pub struct TupleView<'a> {
    pub flags: TypeFlags,
    pub fields: &'a [TypeId],
}

impl<'a> TupleView<'a> {
    fn as_key(self) -> TupleKey<'a> {
        TupleKey { fields: self.fields }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TupleKey<'a> {
    pub fields: &'a [TypeId],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MixedComptimeAndRuntime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompoundKind {
    Struct(StructRef),
    Tuple(TupleRef),
}

struct TupleHeader {
    flags: TypeFlags,
    total_fields: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum Type<'fields> {
    Primitive(PrimitiveType),
    Compound(Compound<'fields>),
}

impl<'a> Type<'a> {
    pub fn flags(&self) -> TypeFlags {
        match self {
            Type::Primitive(p) => p.flags(),
            Type::Compound(c) => c.flags(),
        }
    }
}

pub struct TypeInterner {
    struct_dedup: UnsafeCell<HashTable<StructRef>>,
    tuple_dedup: UnsafeCell<HashTable<TupleRef>>,
    arena: ChunkedArena<MIN_COMPOUND_ALIGN>,
    hasher: DefaultHashBuilder,
    type_name_args: UnsafeCell<ListOfLists<TypeNameArgsId, ValueId>>,
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
/// For compound types the [`ChunkedArena`] offset is stored with spare low bits used as tags.
/// Thanks to the guarantees from [`alloc_append`](ChunkedArena::alloc_append) we know that offsets
/// will be a multiple of our chosen alignment ([`MIN_COMPOUND_ALIGN`]). The lowest bit identifies
/// primitive types and the next bit distinguishes tuple compounds from struct compounds.
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
            TypeId::CBYTES => write!(f, "TypeId::CBYTES"),
            TypeId::NEVER => write!(f, "TypeId::NEVER"),
            compound => write!(f, "TypeId({})", compound.get()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompoundRef(u32);

impl CompoundRef {
    const TUPLE_TAG: u32 = 0b10;

    pub const fn get(self) -> u32 {
        self.0
    }

    const fn new_struct(offset: u32) -> Self {
        CompoundRef(offset)
    }

    const fn new_tuple(offset: u32) -> Self {
        CompoundRef(offset | Self::TUPLE_TAG)
    }

    const fn offset(self) -> u32 {
        self.0 & !Self::TUPLE_TAG
    }

    const fn kind(self) -> CompoundKind {
        if self.0 & Self::TUPLE_TAG == 0 {
            CompoundKind::Struct(StructRef(self))
        } else {
            CompoundKind::Tuple(TupleRef(self))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructRef(CompoundRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TupleRef(CompoundRef);

impl TypeId {
    /// `void` is an alias for the empty tuple `tuple {}`. The empty tuple is pre-interned as the
    /// first arena allocation in [`TypeInterner::new`], so it always lives at offset 0.
    pub const VOID: TypeId = TypeId::from_tuple(TupleRef(CompoundRef::new_tuple(0)));
    pub const U256: TypeId = TypeId::from_primitive(PrimitiveType::U256);
    pub const BOOL: TypeId = TypeId::from_primitive(PrimitiveType::Bool);
    pub const MEMORY_POINTER: TypeId = TypeId::from_primitive(PrimitiveType::MemoryPointer);
    pub const TYPE: TypeId = TypeId::from_primitive(PrimitiveType::Type);
    pub const FUNCTION: TypeId = TypeId::from_primitive(PrimitiveType::Function);
    pub const CBYTES: TypeId = TypeId::from_primitive(PrimitiveType::CBytes);
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
        const { assert!(Self::IS_PRIMITIVE_FLAG < MIN_COMPOUND_ALIGN as u32) };
        let pid = primitive as u32;
        TypeId::new((pid * MIN_COMPOUND_ALIGN as u32) | Self::IS_PRIMITIVE_FLAG)
    }

    pub const fn from_compound(compound: CompoundRef) -> TypeId {
        TypeId::new(compound.0)
    }

    pub const fn from_struct(offset: StructRef) -> TypeId {
        TypeId::from_compound(offset.0)
    }

    pub const fn from_tuple(offset: TupleRef) -> TypeId {
        TypeId::from_compound(offset.0)
    }

    pub const fn is_tuple(self) -> bool {
        match self.as_primitive() {
            Ok(_) => false,
            Err(compound) => matches!(compound.kind(), CompoundKind::Tuple(_)),
        }
    }

    pub const fn is_struct(self) -> bool {
        match self.as_primitive() {
            Ok(_) => false,
            Err(compound) => matches!(compound.kind(), CompoundKind::Struct(_)),
        }
    }

    pub const fn as_primitive(self) -> Result<PrimitiveType, CompoundRef> {
        match self {
            TypeId::U256 => Ok(PrimitiveType::U256),
            TypeId::BOOL => Ok(PrimitiveType::Bool),
            TypeId::MEMORY_POINTER => Ok(PrimitiveType::MemoryPointer),
            TypeId::TYPE => Ok(PrimitiveType::Type),
            TypeId::FUNCTION => Ok(PrimitiveType::Function),
            TypeId::CBYTES => Ok(PrimitiveType::CBytes),
            TypeId::NEVER => Ok(PrimitiveType::Never),
            ty => Err(CompoundRef(ty.get())),
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

const _TYPE_ID_TAGS_OK: () = const {
    assert!(TypeId::IS_PRIMITIVE_FLAG < MIN_COMPOUND_ALIGN as u32);
    assert!(CompoundRef::TUPLE_TAG < MIN_COMPOUND_ALIGN as u32);
    assert!(CompoundRef::TUPLE_TAG & TypeId::IS_PRIMITIVE_FLAG == 0);
};

impl TypeInterner {
    pub fn new() -> Self {
        let interner = Self {
            struct_dedup: UnsafeCell::new(HashTable::new()),
            tuple_dedup: UnsafeCell::new(HashTable::new()),
            arena: ChunkedArena::new(),
            hasher: DefaultHashBuilder::default(),
            type_name_args: UnsafeCell::new(ListOfLists::new()),
        };
        let (empty_tuple, _) = interner.intern_tuple(TupleKey { fields: &[] });
        assert_eq!(
            TypeId::from_tuple(empty_tuple),
            TypeId::VOID,
            "empty tuple must be interned first so `void` lands at arena offset 0"
        );
        interner
    }

    pub fn is_comptime_only(&self, ty: TypeId) -> bool {
        match ty.as_primitive() {
            Ok(prim) => prim.comptime_only(),
            Err(compound) => {
                self.lookup_compound(compound).flags().contains(TypeFlags::COMPTIME_ONLY)
            }
        }
    }

    pub fn intern_struct(
        &self,
        key: StructKey<'_>,
    ) -> (StructRef, Result<(), MixedComptimeAndRuntime>) {
        use std::hash::BuildHasher;

        let hash = self.hasher.hash_one(key);
        // Safety: We only retain the `&mut` reference for the duration of this function and
        // `lookup_struct` and `push_struct` don't reference `self.struct_dedup` at all.
        let dedup = unsafe { &mut (*self.struct_dedup.get()) };
        let entry = dedup.entry(
            hash,
            |&r#struct| self.lookup_struct(r#struct).as_key() == key,
            |&r#struct| self.hasher.hash_one(self.lookup_struct(r#struct).as_key()),
        );

        match entry {
            Entry::Occupied(occupied) => (*occupied.get(), Ok(())),
            Entry::Vacant(vacant_entry) => {
                let (new_ref, ok) = self.push_struct(key);
                vacant_entry.insert(new_ref);
                (new_ref, ok)
            }
        }
    }

    pub fn intern_tuple(
        &self,
        key: TupleKey<'_>,
    ) -> (TupleRef, Result<(), MixedComptimeAndRuntime>) {
        use std::hash::BuildHasher;

        let hash = self.hasher.hash_one(key);
        // Safety: We only retain the `&mut` reference for the duration of this function and
        // `lookup_tuple` and `push_tuple` don't reference `self.tuple_dedup` at all.
        let dedup = unsafe { &mut (*self.tuple_dedup.get()) };
        let entry = dedup.entry(
            hash,
            |&tuple| self.lookup_tuple(tuple).as_key() == key,
            |&tuple| self.hasher.hash_one(self.lookup_tuple(tuple).as_key()),
        );

        match entry {
            Entry::Occupied(occupied) => (*occupied.get(), Ok(())),
            Entry::Vacant(vacant_entry) => {
                let (new_ref, ok) = self.push_tuple(key);
                vacant_entry.insert(new_ref);
                (new_ref, ok)
            }
        }
    }

    pub fn lookup<'s>(&'s self, ty: TypeId) -> Type<'s> {
        match ty.as_primitive() {
            Ok(prim) => Type::Primitive(prim),
            Err(compound) => Type::Compound(self.lookup_compound(compound)),
        }
    }

    fn lookup_compound<'s>(&'s self, compound: CompoundRef) -> Compound<'s> {
        match compound.kind() {
            CompoundKind::Struct(r#ref) => Compound::Struct(self.lookup_struct(r#ref)),
            CompoundKind::Tuple(r#ref) => Compound::Tuple(self.lookup_tuple(r#ref)),
        }
    }

    pub fn lookup_struct<'s>(&'s self, r#struct: StructRef) -> StructView<'s> {
        unsafe {
            let header_ptr = self.arena.get(r#struct.0.offset()) as *const StructHeader;
            let header = &(*header_ptr);
            let fields_start = header_ptr.add(1) as *const Field;

            StructView {
                def_loc: header.def_loc,
                flags: header.flags,
                type_index: header.type_index,
                name: &header.name,
                fields: core::slice::from_raw_parts(fields_start, header.total_fields as usize),
            }
        }
    }

    pub fn lookup_tuple<'s>(&'s self, tuple: TupleRef) -> TupleView<'s> {
        unsafe {
            let header_ptr = self.arena.get(tuple.0.offset()) as *const TupleHeader;
            let header = &(*header_ptr);
            let elements_start = header_ptr.add(1) as *const TypeId;

            TupleView {
                flags: header.flags,
                fields: core::slice::from_raw_parts(elements_start, header.total_fields as usize),
            }
        }
    }

    pub fn try_name_struct_parameterized(&self, ty: TypeId, name: StrId, args: &[ValueId]) {
        let Type::Compound(Compound::Struct(r#struct)) = self.lookup(ty) else {
            return;
        };
        // Deduped structs may be reached through multiple parameterizations; if this
        // TypeId already has a canonical display name, keep it.
        if r#struct.name.get().is_some() {
            return;
        }
        let args = self.intern_type_name_args(args);
        r#struct.name.set(Some(TypeName::Parameterized { name, args }));
    }

    pub fn intern_type_name_args(&self, args: &[ValueId]) -> TypeNameArgsId {
        // SAFETY: We only create this mutable reference for the duration of this call. Callers must
        // not intern type-name args while formatting is holding slices borrowed from this list.
        unsafe { (*self.type_name_args.get()).push_copy_slice(args) }
    }

    fn fmt_struct(
        &self,
        f: &mut impl fmt::Write,
        r#struct: StructRef,
        session: &Session,
        values: &ValueInterner,
    ) -> fmt::Result {
        let view = self.lookup_struct(r#struct);
        if let Some(name) = view.name.get() {
            return match name {
                TypeName::Plain(str_id) => f.write_str(session.lookup_name(str_id)),
                TypeName::Parameterized { name, args } => {
                    f.write_str(session.lookup_name(name))?;
                    f.write_str("(")?;
                    self.fmt_type_name_args(f, args, session, values)?;
                    f.write_str(")")
                }
            };
        }
        let (line, col) = session.offset_to_line_col(view.def_loc.source, view.def_loc.span.start);
        let source = &session.get_source(view.def_loc.source);
        write!(f, "struct@{}:{line}:{col}", source.path.display())
    }

    pub fn fmt_tuple(
        &self,
        f: &mut impl fmt::Write,
        tuple: TupleRef,
        session: &Session,
        values: &ValueInterner,
    ) -> fmt::Result {
        let view = self.lookup_tuple(tuple);
        f.write_str("tuple {")?;
        for (i, &element) in view.fields.iter().enumerate() {
            if i > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{}", self.format(session, values, element))?;
        }
        f.write_str("}")
    }

    fn fmt_type_name_args(
        &self,
        f: &mut impl fmt::Write,
        args: TypeNameArgsId,
        session: &Session,
        values: &ValueInterner,
    ) -> fmt::Result {
        // SAFETY: Formatting only reads type-name args and must not call code that can mutate
        // `type_name_args`; otherwise this borrowed slice could be invalidated by reallocation.
        let args = unsafe { &(&*self.type_name_args.get())[args] };
        let mut sep = "";
        for &arg in args {
            f.write_str(sep)?;
            sep = ", ";
            write!(f, "{}", values.format_value(session, self, arg))?;
        }
        Ok(())
    }

    pub fn format<'a>(
        &'a self,
        sess: &'a Session,
        values: &'a ValueInterner,
        ty: TypeId,
    ) -> FmtType<'a> {
        FmtType { types: self, values, sess, ty }
    }

    fn push_struct(
        &self,
        r#struct: StructKey<'_>,
    ) -> (StructRef, Result<(), MixedComptimeAndRuntime>) {
        let required_space =
            std::mem::size_of::<StructHeader>() + std::mem::size_of_val(r#struct.fields);

        let flags = r#struct
            .fields
            .iter()
            .fold(TypeFlags::NONE, |flags, field| flags | self.lookup(field.ty).flags());

        const {
            assert!(align_of::<StructHeader>() <= MIN_COMPOUND_ALIGN);
            assert!(align_of::<Field>() <= MIN_COMPOUND_ALIGN);
            assert!(align_of::<Field>() <= size_of::<StructHeader>())
        }

        let r#struct = unsafe {
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
                flags,
                type_index: r#struct.type_index,
                name: Cell::new(None),
                total_fields: r#struct.fields.len() as u32,
            });

            debug_assert!(offset.is_multiple_of(MIN_COMPOUND_ALIGN as u32));
            StructRef(CompoundRef::new_struct(offset))
        };
        let mixed = if flags.contains(TypeFlags::UNINITIALIZABLE_MIXED) {
            Err(MixedComptimeAndRuntime)
        } else {
            Ok(())
        };
        (r#struct, mixed)
    }

    fn push_tuple(&self, tuple: TupleKey<'_>) -> (TupleRef, Result<(), MixedComptimeAndRuntime>) {
        let required_space =
            std::mem::size_of::<TupleHeader>() + std::mem::size_of_val(tuple.fields);

        const {
            assert!(align_of::<TupleHeader>() <= MIN_COMPOUND_ALIGN);
            assert!(align_of::<TypeId>() <= MIN_COMPOUND_ALIGN);
            assert!(align_of::<TypeId>() <= align_of::<TupleHeader>());
        }

        let flags = tuple
            .fields
            .iter()
            .fold(TypeFlags::NONE, |flags, element| flags | self.lookup(*element).flags());

        let tuple = unsafe {
            let (offset, new_tuple_ptr) = self.arena.alloc_append(required_space);

            let elements_start = new_tuple_ptr.byte_add(size_of::<TupleHeader>()) as *mut TypeId;
            let mut element_ptr = elements_start;
            for &element in tuple.fields {
                element_ptr.write(element);
                element_ptr = element_ptr.add(1);
            }

            let header_ptr = new_tuple_ptr as *mut TupleHeader;
            header_ptr.write(TupleHeader { flags, total_fields: tuple.fields.len() as u32 });

            debug_assert!(offset.is_multiple_of(MIN_COMPOUND_ALIGN as u32));
            TupleRef(CompoundRef::new_tuple(offset))
        };
        let mixed = if flags.contains(TypeFlags::UNINITIALIZABLE_MIXED) {
            Err(MixedComptimeAndRuntime)
        } else {
            Ok(())
        };
        (tuple, mixed)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Compound<'a> {
    Struct(StructView<'a>),
    Tuple(TupleView<'a>),
}

impl Compound<'_> {
    pub fn field_count(&self) -> usize {
        match self {
            Compound::Struct(r#struct) => r#struct.fields.len(),
            Compound::Tuple(tuple) => tuple.fields.len(),
        }
    }

    pub fn field_type(&self, i: usize) -> TypeId {
        match self {
            Compound::Struct(r#struct) => r#struct.fields[i].ty,
            Compound::Tuple(tuple) => tuple.fields[i],
        }
    }

    pub fn flags(&self) -> TypeFlags {
        match self {
            Compound::Struct(r#struct) => r#struct.flags,
            Compound::Tuple(tuple) => tuple.flags,
        }
    }
}

pub struct FmtType<'a> {
    types: &'a TypeInterner,
    values: &'a ValueInterner,
    sess: &'a Session,
    ty: TypeId,
}

impl std::fmt::Display for FmtType<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.ty.as_primitive() {
            Ok(prim) => write!(f, "{}", prim.name()),
            Err(compound) => match self.types.lookup_compound(compound) {
                Compound::Struct(_) => {
                    self.types.fmt_struct(f, StructRef(compound), self.sess, self.values)
                }
                Compound::Tuple(_) => {
                    self.types.fmt_tuple(f, TupleRef(compound), self.sess, self.values)
                }
            },
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
    use plank_session::{SourceId, SrcLoc, ZERO_SPAN, builtins};

    fn dummy_src_loc(id: u32) -> SrcLoc {
        SrcLoc::new(SourceId::new(id), ZERO_SPAN)
    }

    fn dummy_struct_info(fields: &[Field]) -> StructKey<'_> {
        StructKey { type_index: ValueId::VOID, def_loc: dummy_src_loc(0), fields }
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
        let fields = [Field { name: builtins::U256, ty: TypeId::U256, def_span: ZERO_SPAN }];

        let (a, a_status) = interner.intern_struct(dummy_struct_info(&fields));
        let (b, b_status) = interner.intern_struct(dummy_struct_info(&fields));
        assert_eq!(a, b);
        assert_eq!(a_status, Ok(()));
        assert_eq!(b_status, Ok(()));

        let different = [Field { name: builtins::BOOL, ty: TypeId::BOOL, def_span: ZERO_SPAN }];
        let (c, c_status) = interner.intern_struct(dummy_struct_info(&different));
        assert_ne!(a, c);
        assert_eq!(c_status, Ok(()));
    }

    #[test]
    fn compound_refs_have_aligned_offsets_and_kind_tags() {
        let interner = TypeInterner::new();
        let f = Field { name: builtins::U256, ty: TypeId::U256, def_span: ZERO_SPAN };

        let (a, _) = interner.intern_struct(dummy_struct_info(&[f]));
        let (b, _) = interner.intern_struct(dummy_struct_info(&[f, f]));
        let (c, _) = interner.intern_struct(dummy_struct_info(&[f, f, f]));

        for r#struct in [a, b, c] {
            assert!(matches!(r#struct.0.kind(), CompoundKind::Struct(_)));
            assert!(r#struct.0.offset().is_multiple_of(MIN_COMPOUND_ALIGN as u32));
        }

        let (tuple, _) = interner.intern_tuple(TupleKey { fields: &[TypeId::U256] });
        assert!(matches!(tuple.0.kind(), CompoundKind::Tuple(_)));
        assert!(tuple.0.offset().is_multiple_of(MIN_COMPOUND_ALIGN as u32));
    }

    #[test]
    fn struct_different_src_loc_interns_separately() {
        let interner = TypeInterner::new();
        let fields = [Field { name: builtins::U256, ty: TypeId::U256, def_span: ZERO_SPAN }];

        let a_info =
            StructKey { type_index: ValueId::VOID, def_loc: dummy_src_loc(0), fields: &fields };
        let b_info =
            StructKey { type_index: ValueId::VOID, def_loc: dummy_src_loc(1), fields: &fields };

        let (a, a_status) = interner.intern_struct(a_info);
        let (b, b_status) = interner.intern_struct(b_info);
        assert_ne!(a, b);
        assert_eq!(a_status, Ok(()));
        assert_eq!(b_status, Ok(()));
    }

    #[test]
    fn is_comptime_only_nested_struct() {
        let interner = TypeInterner::new();

        let inner_fields = [Field { name: builtins::U256, ty: TypeId::TYPE, def_span: ZERO_SPAN }];
        let (inner, inner_status) = interner.intern_struct(dummy_struct_info(&inner_fields));
        assert_eq!(inner_status, Ok(()));
        let inner_ty = TypeId::from_struct(inner);
        assert!(interner.is_comptime_only(inner_ty));

        let outer_fields = [Field { name: builtins::BOOL, ty: inner_ty, def_span: ZERO_SPAN }];
        let (outer, outer_status) = interner.intern_struct(dummy_struct_info(&outer_fields));
        assert_eq!(outer_status, Ok(()));
        let outer_ty = TypeId::from_struct(outer);
        assert!(interner.is_comptime_only(outer_ty));

        let runtime_fields =
            [Field { name: builtins::CBYTES, ty: TypeId::U256, def_span: ZERO_SPAN }];
        let (runtime, runtime_status) = interner.intern_struct(dummy_struct_info(&runtime_fields));
        assert_eq!(runtime_status, Ok(()));
        assert!(!interner.is_comptime_only(TypeId::from_struct(runtime)));
    }

    #[test]
    fn tuple_intern_deduplication() {
        let interner = TypeInterner::new();
        let elements = [TypeId::U256, TypeId::BOOL];

        let (a, a_status) = interner.intern_tuple(TupleKey { fields: &elements });
        let (b, b_status) = interner.intern_tuple(TupleKey { fields: &elements });
        assert_eq!(a, b);
        assert_eq!(a_status, Ok(()));
        assert_eq!(b_status, Ok(()));

        let different = [TypeId::BOOL, TypeId::U256];
        let (c, c_status) = interner.intern_tuple(TupleKey { fields: &different });
        assert_ne!(a, c);
        assert_eq!(c_status, Ok(()));
    }

    #[test]
    fn empty_tuple_is_void() {
        let interner = TypeInterner::new();
        let (tuple, status) = interner.intern_tuple(TupleKey { fields: &[] });
        assert_eq!(status, Ok(()));
        let tuple_ty = TypeId::from_tuple(tuple);

        assert_eq!(tuple_ty, TypeId::VOID);
        let Type::Compound(Compound::Tuple(tuple)) = interner.lookup(tuple_ty) else {
            panic!("expected tuple type")
        };
        assert!(tuple.fields.is_empty());
    }

    #[test]
    fn tuple_comptime_only_tracks_elements() {
        let interner = TypeInterner::new();

        let (comptime_tuple, comptime_status) =
            interner.intern_tuple(TupleKey { fields: &[TypeId::TYPE] });
        assert_eq!(comptime_status, Ok(()));
        assert!(interner.is_comptime_only(TypeId::from_tuple(comptime_tuple)));

        let (runtime_tuple, runtime_status) =
            interner.intern_tuple(TupleKey { fields: &[TypeId::U256] });
        assert_eq!(runtime_status, Ok(()));
        assert!(!interner.is_comptime_only(TypeId::from_tuple(runtime_tuple)));
    }

    #[test]
    fn mixed_compound_reports_only_on_first_intern() {
        let interner = TypeInterner::new();
        let fields = [
            Field {
                name: builtins::MEMORY_POINTER,
                ty: TypeId::MEMORY_POINTER,
                def_span: ZERO_SPAN,
            },
            Field { name: builtins::TYPE, ty: TypeId::TYPE, def_span: ZERO_SPAN },
        ];

        let (first_struct, first_struct_status) =
            interner.intern_struct(dummy_struct_info(&fields));
        let (second_struct, second_struct_status) =
            interner.intern_struct(dummy_struct_info(&fields));
        assert_eq!(first_struct, second_struct);
        assert_eq!(first_struct_status, Err(MixedComptimeAndRuntime));
        assert_eq!(second_struct_status, Ok(()));

        let fields = &[TypeId::MEMORY_POINTER, TypeId::TYPE];
        let (first_tuple, first_tuple_status) = interner.intern_tuple(TupleKey { fields });
        let (second_tuple, second_tuple_status) = interner.intern_tuple(TupleKey { fields });
        assert_eq!(first_tuple, second_tuple);
        assert_eq!(first_tuple_status, Err(MixedComptimeAndRuntime));
        assert_eq!(second_tuple_status, Ok(()));
    }

    #[test]
    fn tuple_and_struct_do_not_dedup() {
        let mut sess = Session::new();
        let interner = TypeInterner::new();
        let field = Field { name: sess.intern("name"), ty: TypeId::U256, def_span: ZERO_SPAN };

        let (r#struct, struct_status) = interner.intern_struct(dummy_struct_info(&[field]));
        let (tuple, tuple_status) = interner.intern_tuple(TupleKey { fields: &[TypeId::U256] });
        assert_eq!(struct_status, Ok(()));
        assert_eq!(tuple_status, Ok(()));

        assert_ne!(TypeId::from_struct(r#struct), TypeId::from_tuple(tuple));
    }
}
