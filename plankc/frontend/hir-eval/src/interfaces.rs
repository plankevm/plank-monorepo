use alloy_primitives::U256;
use plank_session::{MaybePoisoned, Poisoned, SourceSpan, StrId};
use plank_values::{Compound, Type, TypeId, Value, ValueId};

use crate::scope::{Diverge, Scope};

pub(crate) enum InterfaceImplError {
    ReturnTypeMismatch { member: StrId, expected: TypeId, actual: TypeId },
    InvalidField { name: StrId, expected: TypeId },
}

impl Scope<'_, '_> {
    fn resolve_std_interface(&mut self, name: StrId, span: SourceSpan) -> MaybePoisoned<TypeId> {
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
    }

    fn resolve_interface_impl(
        &mut self,
        ty: TypeId,
        interface: TypeId,
        span: SourceSpan,
    ) -> MaybePoisoned<Result<ValueId, Diverge>> {
        let method = match self.types.lookup(ty) {
            Type::Compound(Compound::Struct(r#struct)) => {
                self.find_method(r#struct, b"impl")
            }
            _ => None,
        };
        let Some(method) = method else {
            self.diag_ctx.emit_interface_not_implemented(
                self.eval.values,
                ty,
                interface,
                self.loc(span),
            );
            return Err(Poisoned);
        };
        let closure = self.bind_method_self(method, ty);
        let argument = self.eval.values.intern_type(interface);
        let value = match self.eval_synthetic_call(closure, &[argument], span)? {
            Ok(value) => value,
            Err(diverge) => return Ok(Err(diverge)),
        };
        let actual = self.values.type_of_value(value);
        if actual == TypeId::VOID {
            self.diag_ctx.emit_interface_not_implemented(
                self.eval.values,
                ty,
                interface,
                self.loc(span),
            );
            return Err(Poisoned);
        }
        if actual != interface {
            let impl_name = self.diag_ctx.session.intern("impl");
            self.diag_ctx.emit_invalid_interface_impl(
                self.eval.values,
                interface,
                InterfaceImplError::ReturnTypeMismatch {
                    member: impl_name,
                    expected: interface,
                    actual,
                },
                self.loc(span),
            );
            return Err(Poisoned);
        }
        Ok(Ok(value))
    }

    fn get_interface_field(
        &mut self,
        implementation: ValueId,
        name: StrId,
        expected: TypeId,
        span: SourceSpan,
    ) -> MaybePoisoned<ValueId> {
        let (ty, r#struct, fields) = if let Value::Compound { ty, fields } =
            self.values.lookup(implementation)
            && let Type::Compound(Compound::Struct(r#struct)) = self.types.lookup(ty)
        {
            (ty, r#struct, fields)
        } else {
            unreachable!("interface implementation was checked to be a struct")
        };
        if let Some(value) =
            r#struct.fields.iter().position(|field| field.name == name).map(|index| fields[index])
            && self.values.type_of_value(value) == expected
        {
            return Ok(value);
        }
        self.diag_ctx.emit_invalid_interface_impl(
            self.eval.values,
            ty,
            InterfaceImplError::InvalidField { name, expected },
            self.loc(span),
        );
        Err(Poisoned)
    }

    pub(crate) fn eval_as_primitive(
        &mut self,
        value: ValueId,
        span: SourceSpan,
    ) -> MaybePoisoned<Result<(U256, u8), Diverge>> {
        let interface_name = self.diag_ctx.session.intern("AsPrimitive");
        let interface_ty = self.resolve_std_interface(interface_name, span)?;
        let as_primitive = match self.resolve_interface_impl(
            self.values.type_of_value(value),
            interface_ty,
            span,
        )? {
            Ok(value) => value,
            Err(diverge) => return Ok(Err(diverge)),
        };
        let byte_size_name = self.diag_ctx.session.intern("byte_size");
        let byte_size_value =
            self.get_interface_field(as_primitive, byte_size_name, TypeId::U256, span)?;
        let Value::BigNum(byte_size) = self.values.lookup(byte_size_value) else {
            unreachable!("byte_size was checked to be u256")
        };
        let byte_size = match u8::try_from(byte_size) {
            Ok(size) if size <= 32 => size,
            _ => {
                self.diag_ctx.emit_invalid_as_primitive_byte_size(byte_size, self.loc(span));
                return Err(Poisoned);
            }
        };
        let to_raw_name = self.diag_ctx.session.intern("to_raw");
        let to_raw = self.get_interface_field(as_primitive, to_raw_name, TypeId::FUNCTION, span)?;
        let to_raw_result = match self.eval_synthetic_call(to_raw, &[value], span)? {
            Ok(result) => result,
            Err(diverge) => return Ok(Err(diverge)),
        };
        let Value::BigNum(raw) = self.values.lookup(to_raw_result) else {
            self.diag_ctx.emit_invalid_interface_impl(
                self.eval.values,
                interface_ty,
                InterfaceImplError::ReturnTypeMismatch {
                    member: to_raw_name,
                    expected: TypeId::U256,
                    actual: self.values.type_of_value(to_raw_result),
                },
                self.loc(span),
            );
            return Err(Poisoned);
        };
        Ok(Ok((raw, byte_size)))
    }
}
