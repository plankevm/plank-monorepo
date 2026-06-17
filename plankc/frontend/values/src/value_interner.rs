use crate::{
    DefOrigin, FnDefId, Type, TypeId, TypeInterner, ValueId,
    bignum_interner::{BigNumId, BigNumInterner},
};
use alloy_primitives::U256;
use hashbrown::{DefaultHashBuilder, HashMap, HashTable, hash_map, hash_table::Entry};
use plank_core::{IndexVec, list_of_lists::ListOfLists, newtype_index};
use plank_session::{BytesId, Session, SrcLoc, StrId, write_bytes_literal};
use std::{fmt, hash::BuildHasher};

newtype_index! {
    struct CompoundIdx;
    struct CaptureIdx;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CBytes {
    pub contents: BytesId,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoredValue {
    Void,
    Bool(bool),
    BigNum(BigNumId),
    Type(TypeId),
    Bytes(CBytes),
    Closure { fn_def: FnDefId, def_loc: SrcLoc, captures: CaptureIdx },
    StructVal { ty: TypeId, children: CompoundIdx },
    TupleVal { ty: TypeId, children: CompoundIdx },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Value<'a> {
    Void,
    Bool(bool),
    BigNum(U256),
    Type(TypeId),
    Bytes(CBytes),
    Closure { fn_def: FnDefId, def_loc: SrcLoc, captures: &'a [(ValueId, DefOrigin)] },
    StructVal { ty: TypeId, fields: &'a [ValueId] },
    TupleVal { ty: TypeId, elements: &'a [ValueId] },
}

impl Value<'_> {
    pub fn get_type(&self) -> TypeId {
        match self {
            Value::Void => TypeId::VOID,
            Value::Bool(_) => TypeId::BOOL,
            Value::BigNum(_) => TypeId::U256,
            Value::Type(_) => TypeId::TYPE,
            Value::Bytes(_) => TypeId::CBYTES,
            Value::Closure { .. } => TypeId::FUNCTION,
            Value::StructVal { ty, .. } => *ty,
            Value::TupleVal { ty, .. } => *ty,
        }
    }
}

pub struct ValueInterner {
    values: IndexVec<ValueId, StoredValue>,
    dedup: HashTable<ValueId>,
    hasher: DefaultHashBuilder,
    children: ListOfLists<CompoundIdx, ValueId>,
    captures: ListOfLists<CaptureIdx, (ValueId, DefOrigin)>,
    big_nums: BigNumInterner,

    closure_names: HashMap<ValueId, StrId>,
}

impl Default for ValueInterner {
    fn default() -> Self {
        Self::new()
    }
}

fn stored_to_value<'a>(
    stored: StoredValue,
    children: &'a ListOfLists<CompoundIdx, ValueId>,
    captures: &'a ListOfLists<CaptureIdx, (ValueId, DefOrigin)>,
    big_nums: &'a BigNumInterner,
) -> Value<'a> {
    match stored {
        StoredValue::Void => Value::Void,
        StoredValue::Bool(b) => Value::Bool(b),
        StoredValue::BigNum(bid) => Value::BigNum(big_nums.lookup(bid)),
        StoredValue::Type(t) => Value::Type(t),
        StoredValue::Bytes(bytes) => Value::Bytes(bytes),
        StoredValue::Closure { fn_def, def_loc, captures: idx } => {
            Value::Closure { fn_def, def_loc, captures: &captures[idx] }
        }
        StoredValue::StructVal { ty, children: idx } => {
            Value::StructVal { ty, fields: &children[idx] }
        }
        StoredValue::TupleVal { ty, children: idx } => {
            Value::TupleVal { ty, elements: &children[idx] }
        }
    }
}

impl ValueInterner {
    pub fn new() -> Self {
        let mut new_interner = Self {
            values: IndexVec::new(),
            dedup: HashTable::new(),
            hasher: DefaultHashBuilder::default(),
            children: ListOfLists::new(),
            captures: ListOfLists::new(),
            big_nums: BigNumInterner::new(),
            closure_names: HashMap::new(),
        };
        assert_eq!(new_interner.intern(Value::Void), ValueId::VOID);
        assert_eq!(new_interner.intern(Value::Bool(false)), ValueId::FALSE);
        assert_eq!(new_interner.intern(Value::Bool(true)), ValueId::TRUE);
        assert_eq!(new_interner.intern_num(U256::ZERO), ValueId::ZERO_NUM);
        assert_eq!(new_interner.intern_num(U256::ONE), ValueId::ONE_NUM);
        assert_eq!(
            new_interner.intern_bytes(plank_session::EMPTY_BYTES, 0, 0),
            ValueId::BYTES_EMPTY
        );
        new_interner
    }

    pub fn try_name_closure(&mut self, value: ValueId, name: StrId) {
        if let hash_map::Entry::Vacant(vacant) = self.closure_names.entry(value) {
            vacant.insert(name);
        }
    }

    pub fn get_closure_name(&self, closure: ValueId) -> Option<StrId> {
        self.closure_names.get(&closure).copied()
    }

    fn hash_value(&self, value: Value<'_>) -> u64 {
        self.hasher.hash_one(value)
    }

    pub fn type_of_value(&self, value: ValueId) -> TypeId {
        self.lookup(value).get_type()
    }

    pub fn intern_num(&mut self, num: U256) -> ValueId {
        self.intern(Value::BigNum(num))
    }

    pub fn intern_type(&mut self, ty: TypeId) -> ValueId {
        self.intern(Value::Type(ty))
    }

    pub fn intern_bytes(&mut self, contents: BytesId, start: u32, end: u32) -> ValueId {
        self.intern(Value::Bytes(CBytes { contents, start, end }))
    }

    pub fn intern(&mut self, value: Value<'_>) -> ValueId {
        let hash = self.hash_value(value);
        let entry = self.dedup.entry(
            hash,
            |&id| {
                stored_to_value(self.values[id], &self.children, &self.captures, &self.big_nums)
                    == value
            },
            |&id| {
                self.hasher.hash_one(stored_to_value(
                    self.values[id],
                    &self.children,
                    &self.captures,
                    &self.big_nums,
                ))
            },
        );
        match entry {
            Entry::Occupied(occupied) => *occupied.get(),
            Entry::Vacant(vacant) => {
                let stored = match value {
                    Value::Void => StoredValue::Void,
                    Value::Bool(b) => StoredValue::Bool(b),
                    Value::BigNum(n) => StoredValue::BigNum(self.big_nums.intern(n)),
                    Value::Type(t) => StoredValue::Type(t),
                    Value::Bytes(bytes) => StoredValue::Bytes(bytes),
                    Value::Closure { fn_def, def_loc, captures } => StoredValue::Closure {
                        fn_def,
                        def_loc,
                        captures: self.captures.push_copy_slice(captures),
                    },
                    Value::StructVal { ty, fields } => StoredValue::StructVal {
                        ty,
                        children: self.children.push_copy_slice(fields),
                    },
                    Value::TupleVal { ty, elements } => StoredValue::TupleVal {
                        ty,
                        children: self.children.push_copy_slice(elements),
                    },
                };
                let id = self.values.push(stored);
                vacant.insert(id);
                id
            }
        }
    }

    pub fn lookup(&self, id: ValueId) -> Value<'_> {
        stored_to_value(self.values[id], &self.children, &self.captures, &self.big_nums)
    }

    pub fn format_value<'a>(
        &'a self,
        session: &'a Session,
        types: &'a TypeInterner,
        value: ValueId,
    ) -> FmtValue<'a> {
        FmtValue { values: self, session, types, value }
    }
}

pub struct FmtValue<'a> {
    values: &'a ValueInterner,
    session: &'a Session,
    types: &'a TypeInterner,
    value: ValueId,
}

impl FmtValue<'_> {
    fn fmt_value(&self, f: &mut impl fmt::Write, value: ValueId) -> fmt::Result {
        match self.values.lookup(value) {
            Value::Void => f.write_str("{}"),
            Value::Bool(value) => write!(f, "{value}"),
            Value::BigNum(value) => write!(f, "{value}"),
            Value::Bytes(value) => {
                let bytes = self.session.lookup_bytes_slice(value.contents, value.start, value.end);
                write_bytes_literal(f, bytes)
            }
            Value::Type(ty) => write!(f, "{}", self.types.format(self.session, self.values, ty)),
            Value::Closure { def_loc, captures, .. } => {
                let (line, col) =
                    self.session.offset_to_line_col(def_loc.source, def_loc.span.start);
                let source = &self.session.get_source(def_loc.source);
                write!(f, "<closure@{}:{line}:{col}", source.path.display())?;
                if !captures.is_empty() {
                    f.write_str("(")?;
                    let mut sep = "";
                    for &(capture, _) in captures {
                        f.write_str(sep)?;
                        sep = ", ";
                        self.fmt_value(f, capture)?;
                    }
                    f.write_str(")")?;
                }
                f.write_str(">")
            }
            Value::StructVal { ty, fields } => {
                write!(f, "{} {{", self.types.format(self.session, self.values, ty))?;
                let Type::Struct(r#struct) = self.types.lookup(ty) else {
                    unreachable!("invariant: struct value has non-struct type")
                };
                assert_eq!(r#struct.fields.len(), fields.len());
                let mut sep = " ";
                for (&field, &value) in r#struct.fields.iter().zip(fields) {
                    f.write_str(sep)?;
                    sep = ", ";
                    f.write_str(self.session.lookup_name(field.name))?;
                    f.write_str(": ")?;
                    self.fmt_value(f, value)?;
                }
                if fields.is_empty() { f.write_str("}") } else { f.write_str(" }") }
            }
            Value::TupleVal { ty, elements } => {
                write!(f, "{} (", self.types.format(self.session, self.values, ty))?;
                let Type::Tuple(tuple) = self.types.lookup(ty) else {
                    unreachable!("invariant: tuple value has non-tuple type")
                };
                assert_eq!(tuple.elements.len(), elements.len());
                let mut sep = "";
                for &element in elements {
                    f.write_str(sep)?;
                    sep = ", ";
                    self.fmt_value(f, element)?;
                }
                if elements.len() == 1 {
                    f.write_str(",")?;
                }
                f.write_str(")")
            }
        }
    }
}

impl fmt::Display for FmtValue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_value(f, self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::uint;

    #[test]
    fn intern_primitives_dedup() {
        let mut interner = ValueInterner::new();
        let v1 = interner.intern(Value::Void);
        let v2 = interner.intern(Value::Void);
        assert_eq!(v1, v2);

        let b1 = interner.intern(Value::Bool(true));
        let b2 = interner.intern(Value::Bool(true));
        let b3 = interner.intern(Value::Bool(false));
        assert_eq!(b1, b2);
        assert_ne!(b1, b3);
    }

    #[test]
    fn intern_struct_dedup() {
        let mut interner = ValueInterner::new();
        let v1 = interner.intern(Value::Void);
        let ty = interner.intern(Value::Type(TypeId::new(1)));

        let s1 = interner.intern(Value::StructVal { ty: TypeId::new(1), fields: &[v1, ty] });
        let s2 = interner.intern(Value::StructVal { ty: TypeId::new(1), fields: &[v1, ty] });
        assert_eq!(s1, s2);

        let s3 = interner.intern(Value::StructVal { ty: TypeId::new(2), fields: &[v1, ty] });
        assert_ne!(s1, s3);
    }

    #[test]
    fn intern_tuple_dedup() {
        let mut interner = ValueInterner::new();
        let v1 = interner.intern(Value::Void);
        let ty = interner.intern(Value::Type(TypeId::new(1)));

        let t1 = interner.intern(Value::TupleVal { ty: TypeId::new(1), elements: &[v1, ty] });
        let t2 = interner.intern(Value::TupleVal { ty: TypeId::new(1), elements: &[v1, ty] });
        assert_eq!(t1, t2);

        let t3 = interner.intern(Value::TupleVal { ty: TypeId::new(2), elements: &[v1, ty] });
        assert_ne!(t1, t3);
    }

    #[test]
    fn intern_compound_dedup() {
        let mut interner = ValueInterner::new();
        let v1 = interner.intern(Value::Void);
        let ty = TypeId::new(1);
        let ty_value = interner.intern(Value::Type(ty));

        let s = interner.intern(Value::StructVal { ty, fields: &[v1, ty_value] });
        let t = interner.intern(Value::TupleVal { ty, elements: &[v1, ty_value] });

        assert_ne!(s, t);
        assert_eq!(interner.lookup(s), Value::StructVal { ty, fields: &[v1, ty_value] });
        assert_eq!(interner.lookup(t), Value::TupleVal { ty, elements: &[v1, ty_value] });
    }

    #[test]
    fn lookup_roundtrip() {
        let mut interner = ValueInterner::new();
        let v = interner.intern(Value::BigNum(uint!(67_U256)));
        assert_eq!(interner.lookup(v), Value::BigNum(uint!(67_U256)));
    }

    #[test]
    fn intern_bytes_by_identity() {
        let mut session = plank_session::Session::new();
        let mut interner = ValueInterner::new();
        let hello = session.intern_bytes(b"hello");
        let xelx = session.intern_bytes(b"xelx");

        let slice = interner.intern_bytes(hello, 1, 3);
        assert_eq!(interner.intern_bytes(hello, 1, 3), slice);
        assert_ne!(interner.intern_bytes(hello, 1, 4), slice);
        assert_ne!(interner.intern_bytes(hello, 0, 3), slice);

        // "hello"[1..3] and "xelx"[1..3] are both `el`: identical by value but
        // distinct by origin, so they must stay distinct values.
        assert_eq!(session.lookup_bytes_slice(hello, 1, 3), session.lookup_bytes_slice(xelx, 1, 3));
        assert_ne!(interner.intern_bytes(xelx, 1, 3), slice);
    }

    #[test]
    fn intern_num_identical_to_intern() {
        let mut interner = ValueInterner::new();
        let num = uint!(420_U256);
        let via_intern = interner.intern(Value::BigNum(num));
        let via_intern_num = interner.intern_num(num);
        assert_eq!(via_intern, via_intern_num);
    }

    #[test]
    fn intern_type_identical_to_intern() {
        let mut interner = ValueInterner::new();
        let type_id = TypeId::new(7);
        let via_intern = interner.intern(Value::Type(type_id));
        let via_intern_type = interner.intern_type(type_id);
        assert_eq!(via_intern, via_intern_type);
    }
}
