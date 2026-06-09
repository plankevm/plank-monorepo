use plank_core::{chunked_arena::ChunkedArena, list_of_lists::ListOfLists, newtype_index};
use std::{
    cell::{Cell, UnsafeCell},
    fmt,
    mem::{align_of, size_of},
    num::NonZero,
};

use crate::{ValueId, ValueInterner};
use hashbrown::{DefaultHashBuilder, HashSet, HashTable, hash_table::Entry};
use plank_session::{Session, SourceSpan, SrcLoc, StrId};

newtype_index! {
    pub struct TypeNameArgsId;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeName {
    Plain(StrId),
    Parameterized { name: StrId, args: TypeNameArgsId },
}

const _STRUCT_HEADER_FIELD_LAYOUT_OK: () = const {
    assert!(align_of::<StructHeader>() <= MIN_COMPOUND_ALIGN);
    assert!(align_of::<Field>() <= MIN_COMPOUND_ALIGN);
    assert!(size_of::<StructHeader>().is_multiple_of(align_of::<Field>()));
};

const _TUPLE_HEADER_ELEMENT_LAYOUT_OK: () = const {
    assert!(align_of::<TupleHeader>() <= MIN_COMPOUND_ALIGN);
    assert!(align_of::<TypeId>() <= MIN_COMPOUND_ALIGN);
    assert!(size_of::<TupleHeader>().is_multiple_of(align_of::<TypeId>()));
};

const MIN_COMPOUND_ALIGN: usize = {
    let mut align = align_of::<StructHeader>();
    if align_of::<Field>() > align {
        align = align_of::<Field>();
    }
    if align_of::<TupleHeader>() > align {
        align = align_of::<TupleHeader>();
    }
    if align_of::<TypeId>() > align {
        align = align_of::<TypeId>();
    }
    align
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Field {
    pub name: StrId,
    pub ty: TypeId,
    pub def_span: SourceSpan,
}

struct StructHeader {
    def_loc: SrcLoc,
    type_index: ValueId,
    name: Cell<Option<TypeName>>,
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
    pub name: &'a Cell<Option<TypeName>>,
    pub fields: &'a [Field],
}

impl<'a> StructView<'a> {
    fn as_info(self) -> StructInfo<'a> {
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
pub struct TupleView<'a> {
    pub elements: &'a [TypeId],
}

impl<'a> TupleView<'a> {
    fn as_info(self) -> TupleInfo<'a> {
        TupleInfo { elements: self.elements }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TupleInfo<'a> {
    pub elements: &'a [TypeId],
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum CompoundKind {
    Struct,
    Tuple,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum CompoundInfo<'a> {
    Struct(StructInfo<'a>),
    Tuple(TupleInfo<'a>),
}

#[repr(C)]
struct StructHeader {
    kind: CompoundKind,
    def_loc: SrcLoc,
    type_index: ValueId,
    name: Cell<Option<StrId>>,
    total_fields: u32,
}

#[repr(C)]
struct TupleHeader {
    kind: CompoundKind,
    total_elements: u32,
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
    CBytes,
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
            PrimitiveType::CBytes => builtin_names::CBYTES,
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
            PrimitiveType::Type | PrimitiveType::Function | PrimitiveType::CBytes => true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Type<'fields> {
    Primitive(PrimitiveType),
    Struct(StructView<'fields>),
    Tuple(TupleView<'fields>),
}

pub struct TypeInterner {
    comptime_only: UnsafeCell<HashSet<CompoundRef>>,
    dedup: UnsafeCell<HashTable<CompoundRef>>,
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
/// For compound types the [`ChunkedArena`] offset is stored verbatim. Thanks to the guarantees from
/// [`alloc_append`](ChunkedArena::alloc_append) we know that offsets will be a multiple of our
/// chosen alignment ([`MIN_COMPOUND_ALIGN`]). This lets us uniquely identify primitive types
/// by ensuring they are *not* multiples of [`MIN_COMPOUND_ALIGN`], this is done by setting the
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
            TypeId::CBYTES => write!(f, "TypeId::CBYTES"),
            TypeId::NEVER => write!(f, "TypeId::NEVER"),
            compound => write!(f, "TypeId({})", compound.get()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CompoundRef(u32);

impl CompoundRef {
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructRef(CompoundRef);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TupleRef(CompoundRef);

impl TypeId {
    pub const VOID: TypeId = TypeId::from_primitive(PrimitiveType::Void);
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

    pub const fn as_primitive(self) -> Result<PrimitiveType, CompoundRef> {
        match self {
            TypeId::VOID => Ok(PrimitiveType::Void),
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

impl From<StructRef> for TypeId {
    fn from(value: StructRef) -> Self {
        Self::from_struct(value)
    }
}

impl From<TupleRef> for TypeId {
    fn from(value: TupleRef) -> Self {
        Self::from_tuple(value)
    }
}

impl TypeInterner {
    pub fn new() -> Self {
        Self {
            comptime_only: UnsafeCell::new(HashSet::new()),
            arena: ChunkedArena::new(),
            dedup: UnsafeCell::new(HashTable::new()),
            hasher: DefaultHashBuilder::default(),
            type_name_args: UnsafeCell::new(ListOfLists::new()),
        }
    }

    pub fn is_comptime_only(&self, ty: TypeId) -> bool {
        match ty.as_primitive() {
            Ok(prim) => prim.comptime_only(),
            Err(compound) => unsafe { (*self.comptime_only.get()).contains(&compound) },
        }
    }

    pub fn intern_struct(&self, struct_info: StructInfo<'_>) -> StructRef {
        StructRef(self.intern_compound(
            CompoundInfo::Struct(struct_info),
            |this, info| this.push_struct(info).0,
            |this| struct_info.fields.iter().any(|&field| this.is_comptime_only(field.ty)),
        ))
    }

    pub fn intern_tuple(&self, tuple_info: TupleInfo<'_>) -> TupleRef {
        TupleRef(self.intern_compound(
            CompoundInfo::Tuple(tuple_info),
            |this, info| this.push_tuple(info).0,
            |this| tuple_info.elements.iter().any(|&element| this.is_comptime_only(element)),
        ))
    }

    fn intern_compound(
        &self,
        info: CompoundInfo<'_>,
        push: impl FnOnce(&Self, CompoundInfo<'_>) -> CompoundRef,
        is_comptime_only: impl FnOnce(&Self) -> bool,
    ) -> CompoundRef {
        use std::hash::BuildHasher;
        let hash = self.hasher.hash_one(info);
        // Safety: We only retain the `&mut` reference for the duration of this function and
        // `lookup_compound_info` and `push` don't reference `self.dedup` at all.
        let dedup = unsafe { &mut (*self.dedup.get()) };
        let entry = dedup.entry(
            hash,
            |&compound| self.lookup_compound_info(compound) == info,
            |&compound| self.hasher.hash_one(self.lookup_compound_info(compound)),
        );

        match entry {
            Entry::Occupied(occupied) => *occupied.get(),
            Entry::Vacant(vacant_entry) => {
                let new_ref = push(self, info);
                vacant_entry.insert(new_ref);
                if is_comptime_only(self) {
                    unsafe { (*self.comptime_only.get()).insert(new_ref) };
                }
                new_ref
            }
        }
    }

    pub fn lookup<'s>(&'s self, ty: TypeId) -> Type<'s> {
        match ty.as_primitive() {
            Ok(prim) => Type::Primitive(prim),
            Err(compound) => match self.lookup_compound(compound) {
                CompoundView::Struct(view) => Type::Struct(view),
                CompoundView::Tuple(view) => Type::Tuple(view),
            },
        }
    }

    fn compound_kind(&self, compound: CompoundRef) -> CompoundKind {
        unsafe { *(self.arena.get(compound.0) as *const CompoundKind) }
    }

    fn lookup_compound_info<'s>(&'s self, compound: CompoundRef) -> CompoundInfo<'s> {
        match self.lookup_compound(compound) {
            CompoundView::Struct(view) => CompoundInfo::Struct(view.as_info()),
            CompoundView::Tuple(view) => CompoundInfo::Tuple(view.as_info()),
        }
    }

    fn lookup_compound<'s>(&'s self, compound: CompoundRef) -> CompoundView<'s> {
        match self.compound_kind(compound) {
            CompoundKind::Struct => CompoundView::Struct(self.lookup_struct(StructRef(compound))),
            CompoundKind::Tuple => CompoundView::Tuple(self.lookup_tuple(TupleRef(compound))),
        }
    }

    pub fn lookup_struct<'s>(&'s self, r#struct: StructRef) -> StructView<'s> {
        unsafe {
            assert_eq!(self.compound_kind(r#struct.0), CompoundKind::Struct);
            let header_ptr = self.arena.get(r#struct.0.0) as *const StructHeader;
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

    pub fn lookup_tuple<'s>(&'s self, tuple: TupleRef) -> TupleView<'s> {
        unsafe {
            assert_eq!(self.compound_kind(tuple.0), CompoundKind::Tuple);
            let header_ptr = self.arena.get(tuple.0.0) as *const TupleHeader;
            let header = &(*header_ptr);
            let elements_start = header_ptr.add(1) as *const TypeId;

            TupleView {
                elements: core::slice::from_raw_parts(
                    elements_start,
                    header.total_elements as usize,
                ),
            }
        }
    }

    pub fn try_name_struct_parameterized(&self, ty: TypeId, name: StrId, args: &[ValueId]) {
        let Type::Struct(r#struct) = self.lookup(ty) else {
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
        for (i, &element) in view.elements.iter().enumerate() {
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

    fn push_struct<'s, 'a>(&'s self, compound: CompoundInfo<'a>) -> StructRef {
        let CompoundInfo::Struct(r#struct) = compound else { unreachable!() };
        let required_space =
            std::mem::size_of::<StructHeader>() + std::mem::size_of_val(r#struct.fields);

        unsafe {
            let () = _STRUCT_HEADER_FIELD_LAYOUT_OK;
            let (offset, new_struct_ptr) = self.arena.alloc_append(required_space);

            let fields_start = new_struct_ptr.byte_add(size_of::<StructHeader>()) as *mut Field;
            let mut field_ptr = fields_start;
            for &field in r#struct.fields {
                field_ptr.write(field);
                field_ptr = field_ptr.add(1);
            }

            let header_ptr = new_struct_ptr as *mut StructHeader;
            header_ptr.write(StructHeader {
                kind: CompoundKind::Struct,
                def_loc: r#struct.def_loc,
                type_index: r#struct.type_index,
                name: Cell::new(None),
                total_fields: r#struct.fields.len() as u32,
            });

            debug_assert!(offset.is_multiple_of(MIN_COMPOUND_ALIGN as u32));
            StructRef(CompoundRef(offset))
        }
    }

    fn push_tuple<'s, 'a>(&'s self, compound: CompoundInfo<'a>) -> TupleRef {
        let CompoundInfo::Tuple(tuple) = compound else { unreachable!() };
        let required_space =
            std::mem::size_of::<TupleHeader>() + std::mem::size_of_val(tuple.elements);

        unsafe {
            let () = _TUPLE_HEADER_ELEMENT_LAYOUT_OK;
            let (offset, new_tuple_ptr) = self.arena.alloc_append(required_space);

            let elements_start = new_tuple_ptr.byte_add(size_of::<TupleHeader>()) as *mut TypeId;
            let mut element_ptr = elements_start;
            for &element in tuple.elements {
                element_ptr.write(element);
                element_ptr = element_ptr.add(1);
            }

            let header_ptr = new_tuple_ptr as *mut TupleHeader;
            header_ptr.write(TupleHeader {
                kind: CompoundKind::Tuple,
                total_elements: tuple.elements.len() as u32,
            });

            debug_assert!(offset.is_multiple_of(MIN_COMPOUND_ALIGN as u32));
            TupleRef(CompoundRef(offset))
        }
    }
}

enum CompoundView<'a> {
    Struct(StructView<'a>),
    Tuple(TupleView<'a>),
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
                CompoundView::Struct(_) => {
                    self.types.fmt_struct(f, StructRef(compound), self.sess, self.values)
                }
                CompoundView::Tuple(_) => {
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
        let fields = [Field { name: builtins::U256, ty: TypeId::U256, def_span: ZERO_SPAN }];

        let a = interner.intern_struct(dummy_struct_info(&fields));
        let b = interner.intern_struct(dummy_struct_info(&fields));
        assert_eq!(a, b);

        let different = [Field { name: builtins::BOOL, ty: TypeId::BOOL, def_span: ZERO_SPAN }];
        let c = interner.intern_struct(dummy_struct_info(&different));
        assert_ne!(a, c);
    }

    #[test]
    fn struct_refs_are_aligned() {
        let interner = TypeInterner::new();
        let f = Field { name: builtins::U256, ty: TypeId::U256, def_span: ZERO_SPAN };

        let a = interner.intern_struct(dummy_struct_info(&[f]));
        let b = interner.intern_struct(dummy_struct_info(&[f, f]));
        let c = interner.intern_struct(dummy_struct_info(&[f, f, f]));

        for r#struct in [a, b, c] {
            let raw = TypeId::from_struct(r#struct).get();
            assert!(raw.is_multiple_of(MIN_COMPOUND_ALIGN as u32));
        }
    }

    #[test]
    fn struct_different_src_loc_interns_separately() {
        let interner = TypeInterner::new();
        let fields = [Field { name: builtins::U256, ty: TypeId::U256, def_span: ZERO_SPAN }];

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

        let inner_fields = [Field { name: builtins::U256, ty: TypeId::TYPE, def_span: ZERO_SPAN }];
        let inner = interner.intern_struct(dummy_struct_info(&inner_fields));
        let inner_ty = TypeId::from_struct(inner);
        assert!(interner.is_comptime_only(inner_ty));

        let outer_fields = [Field { name: builtins::BOOL, ty: inner_ty, def_span: ZERO_SPAN }];
        let outer = interner.intern_struct(dummy_struct_info(&outer_fields));
        let outer_ty = TypeId::from_struct(outer);
        assert!(interner.is_comptime_only(outer_ty));

        let runtime_fields =
            [Field { name: builtins::CBYTES, ty: TypeId::U256, def_span: ZERO_SPAN }];
        let runtime = interner.intern_struct(dummy_struct_info(&runtime_fields));
        assert!(!interner.is_comptime_only(TypeId::from_struct(runtime)));
    }

    #[test]
    fn tuple_intern_deduplication() {
        let interner = TypeInterner::new();
        let elements = [TypeId::U256, TypeId::BOOL];

        let a = interner.intern_tuple(TupleInfo { elements: &elements });
        let b = interner.intern_tuple(TupleInfo { elements: &elements });
        assert_eq!(a, b);

        let different = [TypeId::BOOL, TypeId::U256];
        let c = interner.intern_tuple(TupleInfo { elements: &different });
        assert_ne!(a, c);
    }

    #[test]
    fn empty_tuple_is_not_void() {
        let interner = TypeInterner::new();
        let tuple = interner.intern_tuple(TupleInfo { elements: &[] });
        let tuple_ty = TypeId::from_tuple(tuple);

        assert_ne!(tuple_ty, TypeId::VOID);
        let Type::Tuple(view) = interner.lookup(tuple_ty) else { panic!("expected tuple type") };
        assert!(view.elements.is_empty());
    }

    #[test]
    fn tuple_comptime_only_tracks_elements() {
        let interner = TypeInterner::new();

        let comptime_tuple = interner.intern_tuple(TupleInfo { elements: &[TypeId::TYPE] });
        assert!(interner.is_comptime_only(TypeId::from_tuple(comptime_tuple)));

        let runtime_tuple = interner.intern_tuple(TupleInfo { elements: &[TypeId::U256] });
        assert!(!interner.is_comptime_only(TypeId::from_tuple(runtime_tuple)));
    }

    #[test]
    fn tuple_and_struct_do_not_dedup() {
        let interner = TypeInterner::new();
        let field = Field { name: StrId::new(0), ty: TypeId::U256, def_span: ZERO_SPAN };

        let r#struct = interner.intern_struct(dummy_struct_info(&[field]));
        let tuple = interner.intern_tuple(TupleInfo { elements: &[TypeId::U256] });

        assert_ne!(TypeId::from_struct(r#struct), TypeId::from_tuple(tuple));
    }
}
