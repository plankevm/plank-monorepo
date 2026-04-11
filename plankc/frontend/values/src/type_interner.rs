use hashbrown::{DefaultHashBuilder, HashSet, HashTable, hash_table::Entry};
use plank_core::{Idx, IndexVec, list_of_lists::ListOfLists, newtype_index};
use plank_session::{Session, SrcLoc, StrId, TypeId, builtins::builtin_names};
use std::{fmt, hash::BuildHasher};

use crate::ValueId;

newtype_index! {
    struct StructIdx;
}

#[derive(Debug, Clone, Copy)]
pub struct StructExtraInfo {
    pub def_loc: SrcLoc,
    pub type_index: ValueId,
    pub name: Option<StrId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructInfo<'a> {
    pub def_loc: SrcLoc,
    pub type_index: ValueId,
    pub fields: &'a [(StrId, TypeId)],
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type<'fields> {
    Void,
    Int,
    Bool,
    MemoryPointer,
    Type,
    Function,
    Never,
    Struct(StructInfo<'fields>),
}

fn get_primitive_id(ty: Type<'_>) -> Result<TypeId, StructInfo<'_>> {
    match ty {
        Type::Void => Ok(TypeId::VOID),
        Type::Int => Ok(TypeId::U256),
        Type::Bool => Ok(TypeId::BOOL),
        Type::MemoryPointer => Ok(TypeId::MEMORY_POINTER),
        Type::Type => Ok(TypeId::TYPE),
        Type::Function => Ok(TypeId::FUNCTION),
        Type::Never => Ok(TypeId::NEVER),
        Type::Struct(r#struct) => Err(r#struct),
    }
}

const fn comptime_only_primitive(ty: TypeId) -> Result<bool, StructIdx> {
    match ty {
        TypeId::VOID | TypeId::U256 | TypeId::BOOL | TypeId::NEVER | TypeId::MEMORY_POINTER => {
            Ok(false)
        }
        TypeId::TYPE | TypeId::FUNCTION => Ok(true),
        _ => Err(StructIdx::new(ty.const_get() - TypeId::STRUCT_IDS_OFFSET)),
    }
}

const fn as_type(ty: TypeId) -> Result<Type<'static>, StructIdx> {
    match ty {
        TypeId::VOID => Ok(Type::Void),
        TypeId::U256 => Ok(Type::Int),
        TypeId::BOOL => Ok(Type::Bool),
        TypeId::MEMORY_POINTER => Ok(Type::MemoryPointer),
        TypeId::TYPE => Ok(Type::Type),
        TypeId::FUNCTION => Ok(Type::Function),
        TypeId::NEVER => Ok(Type::Never),
        _ => Err(StructIdx::new(ty.const_get() - TypeId::STRUCT_IDS_OFFSET)),
    }
}

impl From<StructIdx> for TypeId {
    fn from(value: StructIdx) -> Self {
        Self::new(value.get().wrapping_add(Self::STRUCT_IDS_OFFSET))
    }
}

#[derive(Debug)]
pub struct TypeInterner {
    info_to_struct: HashTable<StructIdx>,
    storage: StructStorage,
}

#[derive(Debug)]
struct StructStorage {
    comptime_only: HashSet<StructIdx>,
    struct_fields: ListOfLists<StructIdx, (StrId, TypeId)>,
    index_to_struct: IndexVec<StructIdx, StructExtraInfo>,
    hasher: DefaultHashBuilder,
}

impl Default for TypeInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeInterner {
    pub fn new() -> Self {
        Self {
            storage: StructStorage {
                comptime_only: HashSet::new(),
                struct_fields: Default::default(),
                index_to_struct: Default::default(),
                hasher: Default::default(),
            },
            info_to_struct: Default::default(),
        }
    }

    pub fn with_capacity(structs: usize, fields: usize) -> Self {
        Self {
            storage: StructStorage {
                comptime_only: HashSet::with_capacity(structs),
                struct_fields: ListOfLists::with_capacities(structs, fields),
                index_to_struct: IndexVec::with_capacity(structs),
                hasher: Default::default(),
            },
            info_to_struct: HashTable::with_capacity(structs),
        }
    }

    pub fn comptime_only(&self, ty: TypeId) -> bool {
        self.storage.comptime_only(ty)
    }

    pub fn intern(&mut self, ty: Type<'_>) -> TypeId {
        let r#struct = match get_primitive_id(ty) {
            Ok(ty) => return ty,
            Err(type_info) => type_info,
        };
        let entry = self.info_to_struct.entry(
            self.storage.hash_struct_info(r#struct),
            |&idx| self.storage.get_info(idx) == r#struct,
            |&idx| self.storage.hash_struct_id(idx),
        );
        match entry {
            Entry::Occupied(occupied) => (*occupied.get()).into(),
            Entry::Vacant(vacant) => {
                let field_struct_idx = self.storage.struct_fields.push_copy_slice(r#struct.fields);
                let new_struct_idx = self.storage.index_to_struct.push(StructExtraInfo {
                    def_loc: r#struct.def_loc,
                    type_index: r#struct.type_index,
                    name: None,
                });

                for &(_name, ty) in r#struct.fields {
                    if self.storage.comptime_only(ty) {
                        self.storage.comptime_only.insert(new_struct_idx);
                        break;
                    }
                }

                assert_eq!(new_struct_idx, field_struct_idx);
                vacant.insert(new_struct_idx);
                new_struct_idx.into()
            }
        }
    }

    pub fn lookup(&self, type_id: TypeId) -> Type<'_> {
        let struct_idx = match as_type(type_id) {
            Ok(ty) => return ty,
            Err(struct_idx) => struct_idx,
        };
        Type::Struct(self.storage.get_info(struct_idx))
    }

    pub fn fmt_type(
        &self,
        f: &mut impl fmt::Write,
        type_id: TypeId,
        session: &Session,
    ) -> fmt::Result {
        match self.lookup(type_id) {
            Type::Void => f.write_str(builtin_names::VOID),
            Type::Int => f.write_str(builtin_names::U256),
            Type::Bool => f.write_str(builtin_names::BOOL),
            Type::MemoryPointer => f.write_str(builtin_names::MEMORY_POINTER),
            Type::Type => f.write_str(builtin_names::TYPE),
            Type::Function => f.write_str(builtin_names::FUNCTION),
            Type::Never => f.write_str(builtin_names::NEVER),
            Type::Struct(info) => match self.struct_name(type_id) {
                Some(name) => f.write_str(session.lookup_name(name)),
                None => {
                    let (line, col) =
                        session.offset_to_line_col(info.def_loc.source, info.def_loc.span.start);
                    write!(
                        f,
                        "struct@{}:{line}:{col}",
                        &session.get_source(info.def_loc.source).path.to_str().unwrap()
                    )
                }
            },
        }
    }

    pub fn type_name(&self, type_id: TypeId, session: &Session) -> String {
        let mut buf = String::with_capacity(16);
        self.fmt_type(&mut buf, type_id, session).unwrap();
        buf
    }

    pub fn field_index_by_name(&self, type_id: TypeId, target_name: StrId) -> Option<u32> {
        let struct_idx = as_type(type_id).err()?;
        self.storage.struct_fields[struct_idx]
            .iter()
            .position(|&(field_name, _ty)| field_name == target_name)
            .map(|i| i as u32)
    }

    pub fn struct_name(&self, type_id: TypeId) -> Option<StrId> {
        let struct_idx = as_type(type_id).err()?;
        self.storage.index_to_struct[struct_idx].name
    }

    pub fn try_set_struct_name(&mut self, type_id: TypeId, name: StrId) -> bool {
        let Some(struct_idx) = as_type(type_id).err() else { return false };
        let extra = &mut self.storage.index_to_struct[struct_idx];
        if extra.name.is_some() {
            return false;
        }
        extra.name = Some(name);
        true
    }

    pub fn format<'a>(&'a self, sess: &'a Session, ty: TypeId) -> FmtType<'a> {
        FmtType { types: self, sess, ty }
    }
}

impl StructStorage {
    fn get_info(&self, idx: StructIdx) -> StructInfo<'_> {
        let stored = &self.index_to_struct[idx];
        StructInfo {
            def_loc: stored.def_loc,
            type_index: stored.type_index,
            fields: &self.struct_fields[idx],
        }
    }

    fn hash_struct_id(&self, idx: StructIdx) -> u64 {
        self.hash_struct_info(self.get_info(idx))
    }

    fn hash_struct_info(&self, r#struct: StructInfo) -> u64 {
        self.hasher.hash_one(r#struct)
    }

    pub fn comptime_only(&self, ty: TypeId) -> bool {
        let struct_idx = match comptime_only_primitive(ty) {
            Ok(comptime_only) => return comptime_only,
            Err(struct_idx) => struct_idx,
        };
        self.comptime_only.contains(&struct_idx)
    }
}

pub struct FmtType<'a> {
    types: &'a TypeInterner,
    sess: &'a Session,
    ty: TypeId,
}

impl std::fmt::Display for FmtType<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.types.fmt_type(f, self.ty, self.sess)
    }
}
