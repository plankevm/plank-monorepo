use alloy_primitives::U256;
use hashbrown::HashMap;
use plank_core::{Idx, RelSlice};
use plank_session::{MaybePoisoned, Poisoned, Session, SourceSpan, SrcLoc, StrId};
use plank_values::{Compound, Type, TypeId, Value, ValueId, ValueIdx};

use crate::scope::{Diverge, Scope};

trait CompilerInterface: Sized {
    fn validate_definition(
        scope: &mut Scope<'_, '_>,
        interface: TypeId,
    ) -> MaybePoisoned<InterfaceDefinition>;

    fn decode(
        scope: &mut Scope<'_, '_>,
        implementation: ValueId,
        span: SourceSpan,
    ) -> MaybePoisoned<Self>;
}

#[derive(Clone, Copy)]
enum CachedInterface {
    Implemented(ValueId),
    NotImplemented,
    InvalidReturnType(TypeId),
}

struct InterfaceDefinition {
    member_indices: HashMap<StrId, ValueIdx>,
}

pub(crate) struct InterfaceCache {
    resolved_names: HashMap<StrId, MaybePoisoned<TypeId>>,
    definitions: HashMap<TypeId, MaybePoisoned<InterfaceDefinition>>,
    implementations: HashMap<(TypeId, TypeId), CachedInterface>,

    impl_name: StrId,

    // `AsPrimitive`
    as_primitive_name: StrId,
    byte_size_name: StrId,
    to_raw_name: StrId,
    unchecked_from_raw_name: StrId,
}

impl InterfaceCache {
    pub(crate) fn new(session: &mut Session) -> Self {
        Self {
            resolved_names: HashMap::new(),
            definitions: HashMap::new(),
            implementations: HashMap::new(),

            impl_name: session.intern("impl"),

            as_primitive_name: session.intern("AsPrimitive"),
            byte_size_name: session.intern("byte_size"),
            to_raw_name: session.intern("to_raw"),
            unchecked_from_raw_name: session.intern("unchecked_from_raw"),
        }
    }
}

impl Scope<'_, '_> {
    fn resolve_interface<I: CompilerInterface>(
        &mut self,
        interface: TypeId,
        ty: TypeId,
        span: SourceSpan,
    ) -> MaybePoisoned<Result<I, Diverge>> {
        let cached = match self.eval.interface_cache.implementations.get(&(interface, ty)) {
            Some(&cached) => cached,
            None => match self.cache_interface::<I>(interface, ty, span)? {
                Ok(cached) => cached,
                Err(diverge) => return Ok(Err(diverge)),
            },
        };
        let implementation = match cached {
            CachedInterface::Implemented(value) => value,
            CachedInterface::NotImplemented => {
                self.diag_ctx.emit_interface_not_implemented(
                    self.eval.values,
                    ty,
                    interface,
                    self.loc(span),
                );
                return Err(Poisoned);
            }
            CachedInterface::InvalidReturnType(actual) => {
                self.diag_ctx.emit_interface_return_type_mismatch(
                    self.eval.values,
                    interface,
                    self.eval.interface_cache.impl_name,
                    interface,
                    actual,
                    self.loc(span),
                );
                return Err(Poisoned);
            }
        };
        I::decode(self, implementation, span).map(Ok)
    }

    fn interface_member(&self, implementation: ValueId, name: StrId) -> ValueId {
        let Value::Compound { ty, fields } = self.eval.values.lookup(implementation) else {
            unreachable!("invariant: interface implementation was checked to be a struct")
        };
        let definition = self.eval.interface_cache.definitions[&ty].as_ref().expect(
            "invariant: interface definition was validated before decoding its implementation",
        );
        let values = RelSlice::new(ValueIdx::ZERO, fields);
        values[definition.member_indices[&name]]
    }

    fn cache_interface_definition<I: CompilerInterface>(
        &mut self,
        interface: TypeId,
    ) -> MaybePoisoned<()> {
        if let Some(definition) = self.eval.interface_cache.definitions.get(&interface) {
            return definition.as_ref().map(|_| ()).map_err(|_| Poisoned);
        }
        let definition = I::validate_definition(self, interface);
        let validity = definition.as_ref().map(|_| ()).map_err(|_| Poisoned);
        self.eval.interface_cache.definitions.insert(interface, definition);
        validity
    }

    fn resolve_std_interface(&mut self, name: StrId, span: SourceSpan) -> MaybePoisoned<TypeId> {
        if let Some(&resolved) = self.eval.interface_cache.resolved_names.get(&name) {
            return resolved;
        }
        let resolved = (|| {
            let const_id = self.eval.core.interfaces.and_then(|source| {
                self.hir.consts.iter_idx().find(|&id| {
                    let def = self.hir.consts[id];
                    def.source_id == source && def.name == name
                })
            });
            let Some(const_id) = const_id else {
                self.diag_ctx.emit_cannot_resolve_std_interface(name, self.loc(span));
                return Err(Poisoned);
            };
            let value = self.eval.evaluate_const(const_id, self.diag_ctx)?;
            if let Value::Type(ty) = self.values.lookup(value)
                && ty.is_struct()
            {
                return Ok(ty);
            }
            self.diag_ctx.emit_cannot_resolve_std_interface(name, self.hir.consts[const_id].loc());
            Err(Poisoned)
        })();
        self.eval.interface_cache.resolved_names.insert(name, resolved);
        resolved
    }

    fn validate_interface_definition(
        &mut self,
        interface: TypeId,
        required_members: &[(StrId, TypeId)],
    ) -> MaybePoisoned<InterfaceDefinition> {
        let Type::Compound(Compound::Struct(r#struct)) = self.types.lookup(interface) else {
            unreachable!("invariant: interface was checked to be a struct type")
        };
        let fields = RelSlice::<ValueIdx, _>::new(ValueIdx::ZERO, r#struct.fields);
        let member_indices: HashMap<_, _> =
            fields.enumerate_idx().map(|(index, field)| (field.name, index)).collect();
        let mut invalid = false;
        for &(name, expected) in required_members {
            let field = member_indices.get(&name).map(|&index| fields[index]);
            if field.is_none_or(|field| field.ty != expected) {
                let loc = field.map_or(r#struct.def_loc, |field| {
                    SrcLoc::new(r#struct.def_loc.source, field.def_span)
                });
                self.diag_ctx.emit_invalid_interface_definition_field(
                    self.eval.values,
                    interface,
                    name,
                    expected,
                    field.map(|field| field.ty),
                    loc,
                );
                invalid = true;
            }
        }
        if invalid {
            return Err(Poisoned);
        }
        Ok(InterfaceDefinition { member_indices })
    }

    fn cache_interface<I: CompilerInterface>(
        &mut self,
        interface: TypeId,
        ty: TypeId,
        span: SourceSpan,
    ) -> MaybePoisoned<Result<CachedInterface, Diverge>> {
        self.cache_interface_definition::<I>(interface)?;
        let method = match self.types.lookup(ty) {
            Type::Compound(Compound::Struct(r#struct)) => {
                self.find_method(r#struct, self.eval.interface_cache.impl_name)
            }
            _ => None,
        };
        let implementation = if let Some(method) = method {
            let closure = self.bind_method_self(method, ty);
            let argument = self.eval.values.intern_type(interface);
            let value = match self.eval_synthetic_call(closure, &[argument], span)? {
                Ok(value) => value,
                Err(diverge) => return Ok(Err(diverge)),
            };
            match self.values.type_of_value(value) {
                TypeId::VOID => CachedInterface::NotImplemented,
                actual if actual == interface => CachedInterface::Implemented(value),
                actual => CachedInterface::InvalidReturnType(actual),
            }
        } else {
            CachedInterface::NotImplemented
        };
        self.eval.interface_cache.implementations.insert((interface, ty), implementation);
        Ok(Ok(implementation))
    }
}

struct AsPrimitive {
    byte_size: u8,
    to_raw: ValueId,
}

impl CompilerInterface for AsPrimitive {
    fn validate_definition(
        scope: &mut Scope<'_, '_>,
        interface: TypeId,
    ) -> MaybePoisoned<InterfaceDefinition> {
        let required_members = [
            (scope.eval.interface_cache.byte_size_name, TypeId::U256),
            (scope.eval.interface_cache.to_raw_name, TypeId::FUNCTION),
            (scope.eval.interface_cache.unchecked_from_raw_name, TypeId::FUNCTION),
        ];
        scope.validate_interface_definition(interface, &required_members)
    }

    fn decode(
        scope: &mut Scope<'_, '_>,
        implementation: ValueId,
        span: SourceSpan,
    ) -> MaybePoisoned<Self> {
        let byte_size_value =
            scope.interface_member(implementation, scope.eval.interface_cache.byte_size_name);
        let Value::BigNum(byte_size) = scope.values.lookup(byte_size_value) else {
            unreachable!("invariant: interface definition validated byte_size as u256")
        };
        let byte_size = match u8::try_from(byte_size) {
            Ok(size) if size <= 32 => size,
            _ => {
                scope.diag_ctx.emit_invalid_as_primitive_byte_size(byte_size, scope.loc(span));
                return Err(Poisoned);
            }
        };
        let to_raw = scope.interface_member(implementation, scope.eval.interface_cache.to_raw_name);
        Ok(Self { byte_size, to_raw })
    }
}

impl Scope<'_, '_> {
    pub(crate) fn eval_as_primitive(
        &mut self,
        value: ValueId,
        span: SourceSpan,
    ) -> MaybePoisoned<Result<(U256, u8), Diverge>> {
        let interface =
            self.resolve_std_interface(self.eval.interface_cache.as_primitive_name, span)?;
        let implementation = match self.resolve_interface::<AsPrimitive>(
            interface,
            self.values.type_of_value(value),
            span,
        )? {
            Ok(implementation) => implementation,
            Err(diverge) => return Ok(Err(diverge)),
        };
        let to_raw_result = match self.eval_synthetic_call(implementation.to_raw, &[value], span)? {
            Ok(result) => result,
            Err(diverge) => return Ok(Err(diverge)),
        };
        let Value::BigNum(raw) = self.values.lookup(to_raw_result) else {
            self.diag_ctx.emit_interface_return_type_mismatch(
                self.eval.values,
                interface,
                self.eval.interface_cache.to_raw_name,
                TypeId::U256,
                self.values.type_of_value(to_raw_result),
                self.loc(span),
            );
            return Err(Poisoned);
        };
        if raw.bit_len() > usize::from(implementation.byte_size) * 8 {
            self.diag_ctx.emit_as_primitive_raw_out_of_range(
                raw,
                implementation.byte_size,
                self.loc(span),
            );
            return Err(Poisoned);
        }
        Ok(Ok((raw, implementation.byte_size)))
    }
}
