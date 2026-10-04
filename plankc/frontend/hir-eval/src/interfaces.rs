use alloy_primitives::U256;
use hashbrown::HashMap;
use plank_core::Span;
use plank_session::{MaybePoisoned, Poisoned, SourceId, SourceSpan, SrcLoc, StrId};
use plank_values::{Compound, FieldIdx, Type, TypeId, Value, ValueId, ValueInterner};

use crate::{
    diagnostics::DiagCtx,
    evaluator::Evaluator,
    scope::{Diverge, Scope},
};

#[derive(Clone, Copy)]
enum ImplStatus {
    Valid(AsPrimitiveImpl),
    Missing,
    Poisoned,
}

#[derive(Default)]
pub(crate) struct StdInterfaces {
    // `AsPrimitive`
    as_primitive_def: Option<MaybePoisoned<AsPrimitiveDef>>,
    as_primitive_impls: HashMap<TypeId, ImplStatus>,
}

impl StdInterfaces {
    pub(crate) fn from_std<'a>(
        core_interfaces_source: SourceId,
        evaluator: &mut Evaluator<'a>,
        diag_ctx: &mut DiagCtx<'a>,
    ) -> Self {
        Self {
            as_primitive_def: Some(resolve_as_primitive_def(
                core_interfaces_source,
                evaluator,
                diag_ctx,
            )),
            ..Self::default()
        }
    }

    pub(crate) fn as_primitive_def(&self) -> Option<MaybePoisoned<AsPrimitiveDef>> {
        self.as_primitive_def
    }
}

impl Scope<'_, '_> {
    pub(crate) fn eval_as_primitive_to_raw(
        &mut self,
        def: AsPrimitiveDef,
        value: ValueId,
        span: SourceSpan,
    ) -> MaybePoisoned<Result<(U256, u8), Diverge>> {
        let as_primitive_impl =
            match self.lookup_as_primitive_impl(def, self.values.type_of_value(value), span)? {
                Ok(as_primitive_impl) => as_primitive_impl,
                Err(diverge) => return Ok(Err(diverge)),
            };
        let to_raw_result =
            match self.eval_synthetic_comptime_call(as_primitive_impl.to_raw, &[value], span)? {
                Ok(result) => result,
                Err(diverge) => return Ok(Err(diverge)),
            };
        let Value::BigNum(raw) = self.values.lookup(to_raw_result) else {
            self.diag_ctx.emit_as_primitive_to_raw_return_type_mismatch(
                self.eval.values,
                def.ty,
                self.values.type_of_value(to_raw_result),
                self.loc(span),
            );
            return Err(Poisoned);
        };
        if raw.bit_len() > usize::from(as_primitive_impl.byte_size) * 8 {
            self.diag_ctx.emit_as_primitive_raw_exceeds_byte_size(
                raw,
                as_primitive_impl.byte_size,
                self.loc(span),
            );
            return Err(Poisoned);
        }
        Ok(Ok((raw, as_primitive_impl.byte_size)))
    }

    fn lookup_as_primitive_impl(
        &mut self,
        def: AsPrimitiveDef,
        ty: TypeId,
        span: SourceSpan,
    ) -> MaybePoisoned<Result<AsPrimitiveImpl, Diverge>> {
        let status = match self.eval.std_interfaces.as_primitive_impls.get(&ty) {
            Some(&status) => status,
            None => {
                let status = match self.compute_as_primitive_impl(def, ty, span) {
                    Ok(status) => status,
                    Err(diverge) => return Ok(Err(diverge)),
                };
                self.eval.std_interfaces.as_primitive_impls.insert(ty, status);
                status
            }
        };
        match status {
            ImplStatus::Valid(as_primitive_impl) => Ok(Ok(as_primitive_impl)),
            ImplStatus::Missing => {
                self.diag_ctx.emit_interface_not_implemented(
                    self.eval.values,
                    ty,
                    def.ty,
                    self.loc(span),
                );
                Err(Poisoned)
            }
            ImplStatus::Poisoned => Err(Poisoned),
        }
    }

    fn compute_as_primitive_impl(
        &mut self,
        def: AsPrimitiveDef,
        ty: TypeId,
        span: SourceSpan,
    ) -> Result<ImplStatus, Diverge> {
        let impl_name = self.diag_ctx.session.intern("impl");
        let method = match self.types.lookup(ty) {
            Type::Compound(Compound::Struct(r#struct)) => {
                self.find_method(r#struct, impl_name.into())
            }
            _ => None,
        };
        let Some(method) = method else {
            return Ok(ImplStatus::Missing);
        };
        let Value::Closure { fn_def, .. } = self.values.lookup(method.closure) else {
            unreachable!("invariant: method definitions always contain closures")
        };
        let fn_def = self.hir.fns[fn_def];
        let impl_loc = fn_def.loc(Span::new(fn_def.source_span.start, fn_def.param_list_span.end));
        let use_loc = self.loc(span);

        let closure = self.bind_method_self(method, ty);
        let argument = self.eval.values.intern_type(def.ty);
        let value = match self.eval_synthetic_comptime_call(closure, &[argument], span) {
            Ok(Ok(value)) => value,
            Ok(Err(diverge)) => return Err(diverge),
            Err(Poisoned) => return Ok(ImplStatus::Poisoned),
        };
        match self.values.type_of_value(value) {
            TypeId::VOID => Ok(ImplStatus::Missing),
            actual if actual == def.ty => {
                match decode_as_primitive_impl(self.eval.values, def, value) {
                    Ok(as_primitive_impl) => Ok(ImplStatus::Valid(as_primitive_impl)),
                    Err(byte_size) => {
                        self.diag_ctx.emit_invalid_as_primitive_byte_size(
                            self.eval.values,
                            ty,
                            def.ty,
                            byte_size,
                            impl_loc,
                            use_loc,
                        );
                        Ok(ImplStatus::Poisoned)
                    }
                }
            }
            actual => {
                self.diag_ctx.emit_interface_impl_return_type_mismatch(
                    self.eval.values,
                    ty,
                    def.ty,
                    actual,
                    impl_loc,
                    use_loc,
                );
                Ok(ImplStatus::Poisoned)
            }
        }
    }
}

#[derive(Clone, Copy)]
struct AsPrimitiveImpl {
    byte_size: u8,
    to_raw: ValueId,
}

fn decode_as_primitive_impl(
    values: &ValueInterner,
    def: AsPrimitiveDef,
    impl_value: ValueId,
) -> Result<AsPrimitiveImpl, U256> {
    let Value::Compound { fields, .. } = values.lookup(impl_value) else {
        unreachable!("invariant: interface implementation was checked to be a struct")
    };
    let to_raw = fields[def.to_raw];
    let Value::BigNum(byte_size) = values.lookup(fields[def.byte_size]) else {
        unreachable!("invariant: interface definition validated byte_size as u256")
    };
    match u8::try_from(byte_size) {
        Ok(byte_size) if byte_size <= 32 => Ok(AsPrimitiveImpl { byte_size, to_raw }),
        _ => Err(byte_size),
    }
}

#[derive(Clone, Copy)]
pub(crate) struct AsPrimitiveDef {
    ty: TypeId,
    byte_size: FieldIdx,
    to_raw: FieldIdx,
}

fn resolve_as_primitive_def<'a>(
    source: SourceId,
    evaluator: &mut Evaluator<'a>,
    diag_ctx: &mut DiagCtx<'a>,
) -> MaybePoisoned<AsPrimitiveDef> {
    let session = &mut *diag_ctx.session;
    let name = session.intern("AsPrimitive");
    let byte_size = session.intern("byte_size");
    let to_raw = session.intern("to_raw");
    let unchecked_from_raw = session.intern("unchecked_from_raw");

    let hir = evaluator.hir;
    let Some(const_id) = hir.find_const(source, name) else {
        diag_ctx.emit_failed_to_resolve_std_interface(source, name);
        return Err(Poisoned);
    };
    let value = evaluator.evaluate_const(const_id, diag_ctx)?;
    let interface = match evaluator.values.lookup(value) {
        Value::Type(ty) => match evaluator.types.lookup(ty) {
            Type::Compound(Compound::Struct(r#struct)) => Some((ty, r#struct)),
            _ => None,
        },
        _ => None,
    };
    let Some((ty, r#struct)) = interface else {
        diag_ctx.emit_std_interface_not_a_struct(name, hir.consts[const_id].loc());
        return Err(Poisoned);
    };

    let fields = r#struct.fields;
    let mut required_field = |name: StrId, expected: TypeId| {
        let found = fields.enumerate_idx().find(|(_, field)| field.name == name);
        let valid = found.filter(|(_, field)| field.ty == expected).map(|(index, _)| index);
        if valid.is_none() {
            let loc = found.map_or(r#struct.def_loc, |(_, field)| {
                SrcLoc::new(r#struct.def_loc.source, field.def_span)
            });
            diag_ctx.emit_invalid_interface_definition_field(
                evaluator.values,
                ty,
                name,
                expected,
                found.map(|(_, field)| field.ty),
                loc,
            );
        }
        valid
    };
    let byte_size = required_field(byte_size, TypeId::U256);
    let to_raw = required_field(to_raw, TypeId::FUNCTION);
    let unchecked_from_raw = required_field(unchecked_from_raw, TypeId::FUNCTION);
    let (Some(byte_size), Some(to_raw), Some(_)) = (byte_size, to_raw, unchecked_from_raw) else {
        return Err(Poisoned);
    };
    Ok(AsPrimitiveDef { ty, byte_size, to_raw })
}
