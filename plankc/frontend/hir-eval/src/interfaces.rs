use alloy_primitives::U256;
use hashbrown::HashMap;
use plank_core::{Idx, RelSlice};
use plank_session::{MaybePoisoned, Poisoned, SourceSpan, SrcLoc, StrId};
use plank_values::{Compound, Type, TypeId, Value, ValueId, ValueIdx};

use crate::scope::{Diverge, Scope};

#[derive(Clone, Copy)]
enum CachedImplementation {
    Implemented(ValueId),
    NotImplemented,
}

pub(crate) struct CachedInterface {
    members: HashMap<StrId, ValueIdx>,
    implementations: HashMap<TypeId, CachedImplementation>,
}

impl Scope<'_, '_> {
    fn resolve_std_interface(
        &mut self,
        name: StrId,
        required_fields: &[(&str, TypeId)],
        span: SourceSpan,
    ) -> MaybePoisoned<TypeId> {
        if let Some(&resolved) = self.eval.resolved_core_interfaces.get(&name) {
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
                self.cache_interface_definition(ty, required_fields)?;
                return Ok(ty);
            }
            self.diag_ctx.emit_cannot_resolve_std_interface(name, self.hir.consts[const_id].loc());
            Err(Poisoned)
        })();
        self.eval.resolved_core_interfaces.insert(name, resolved);
        resolved
    }

    fn cache_interface_definition(
        &mut self,
        interface: TypeId,
        required_fields: &[(&str, TypeId)],
    ) -> MaybePoisoned<()> {
        if self.eval.interfaces.contains_key(&interface) {
            return Ok(());
        }
        let Type::Compound(Compound::Struct(r#struct)) = self.types.lookup(interface) else {
            unreachable!("interface was checked to be a struct type")
        };
        let fields = RelSlice::<ValueIdx, _>::new(ValueIdx::ZERO, r#struct.fields);
        let members: HashMap<_, _> =
            fields.enumerate_idx().map(|(index, field)| (field.name, index)).collect();
        let mut invalid = false;
        for &(name, expected) in required_fields {
            let name = self.diag_ctx.session.intern(name);
            let field = members.get(&name).map(|&index| fields[index]);
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
        self.eval
            .interfaces
            .insert(interface, CachedInterface { members, implementations: HashMap::new() });
        Ok(())
    }

    fn resolve_interface_impl(
        &mut self,
        ty: TypeId,
        interface: TypeId,
        span: SourceSpan,
        validate: impl FnOnce(&mut Self, ValueId) -> MaybePoisoned<()>,
    ) -> MaybePoisoned<Result<CachedImplementation, Diverge>> {
        if let Some(&cached) = self
            .eval
            .interfaces
            .get(&interface)
            .expect("invariant: interface definition is cached before resolving implementations")
            .implementations
            .get(&ty)
        {
            return Ok(Ok(cached));
        }
        let impl_name = self.diag_ctx.session.intern("impl");
        let method = match self.types.lookup(ty) {
            Type::Compound(Compound::Struct(r#struct)) => self.find_method(r#struct, impl_name),
            _ => None,
        };
        let cached = if let Some(method) = method {
            let closure = self.bind_method_self(method, ty);
            let argument = self.eval.values.intern_type(interface);
            let value = match self.eval_synthetic_call(closure, &[argument], span)? {
                Ok(value) => value,
                Err(diverge) => return Ok(Err(diverge)),
            };
            let actual = self.values.type_of_value(value);
            if actual != interface && actual != TypeId::VOID {
                self.diag_ctx.emit_interface_return_type_mismatch(
                    self.eval.values,
                    interface,
                    impl_name,
                    interface,
                    actual,
                    self.loc(span),
                );
                return Err(Poisoned);
            }
            if actual == TypeId::VOID {
                CachedImplementation::NotImplemented
            } else {
                validate(self, value)?;
                CachedImplementation::Implemented(value)
            }
        } else {
            CachedImplementation::NotImplemented
        };
        self.eval
            .interfaces
            .get_mut(&interface)
            .expect("invariant: interface definition is cached before resolving implementations")
            .implementations
            .insert(ty, cached);
        Ok(Ok(cached))
    }

    fn get_interface_field(&self, implementation: ValueId, name: StrId) -> ValueId {
        let Value::Compound { ty, fields } = self.values.lookup(implementation) else {
            unreachable!("invariant: interface implementation was checked to be a struct")
        };
        let members = RelSlice::new(ValueIdx::ZERO, fields);
        let index = self.eval.interfaces[&ty].members[&name];
        members[index]
    }

    pub(crate) fn eval_as_primitive(
        &mut self,
        value: ValueId,
        span: SourceSpan,
    ) -> MaybePoisoned<Result<(U256, u8), Diverge>> {
        let interface_name = self.diag_ctx.session.intern("AsPrimitive");
        let interface_ty = self.resolve_std_interface(
            interface_name,
            &[
                ("byte_size", TypeId::U256),
                ("to_raw", TypeId::FUNCTION),
                ("unchecked_from_raw", TypeId::FUNCTION),
            ],
            span,
        )?;
        let byte_size_name = self.diag_ctx.session.intern("byte_size");
        let as_primitive = match self.resolve_interface_impl(
            self.values.type_of_value(value),
            interface_ty,
            span,
            |this, implementation| {
                let byte_size_value = this.get_interface_field(implementation, byte_size_name);
                let Value::BigNum(byte_size) = this.values.lookup(byte_size_value) else {
                    unreachable!("invariant: interface definition validated byte_size as u256")
                };
                if byte_size > U256::from(32) {
                    this.diag_ctx.emit_invalid_as_primitive_byte_size(byte_size, this.loc(span));
                    return Err(Poisoned);
                }
                Ok(())
            },
        )? {
            Ok(CachedImplementation::Implemented(value)) => value,
            Ok(CachedImplementation::NotImplemented) => {
                self.diag_ctx.emit_interface_not_implemented(
                    self.eval.values,
                    self.values.type_of_value(value),
                    interface_ty,
                    self.loc(span),
                );
                return Err(Poisoned);
            }
            Err(diverge) => return Ok(Err(diverge)),
        };
        let byte_size_value = self.get_interface_field(as_primitive, byte_size_name);
        let Value::BigNum(byte_size) = self.values.lookup(byte_size_value) else {
            unreachable!("byte_size was checked to be u256")
        };
        let byte_size = u8::try_from(byte_size)
            .expect("invariant: AsPrimitive byte_size was validated before caching");
        let to_raw_name = self.diag_ctx.session.intern("to_raw");
        let to_raw = self.get_interface_field(as_primitive, to_raw_name);
        let to_raw_result = match self.eval_synthetic_call(to_raw, &[value], span)? {
            Ok(result) => result,
            Err(diverge) => return Ok(Err(diverge)),
        };
        let Value::BigNum(raw) = self.values.lookup(to_raw_result) else {
            self.diag_ctx.emit_interface_return_type_mismatch(
                self.eval.values,
                interface_ty,
                to_raw_name,
                TypeId::U256,
                self.values.type_of_value(to_raw_result),
                self.loc(span),
            );
            return Err(Poisoned);
        };
        if raw.bit_len() > usize::from(byte_size) * 8 {
            self.diag_ctx.emit_as_primitive_raw_out_of_range(raw, byte_size, self.loc(span));
            return Err(Poisoned);
        }
        Ok(Ok((raw, byte_size)))
    }
}
