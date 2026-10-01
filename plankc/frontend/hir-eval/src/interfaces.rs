use alloy_primitives::U256;
use hashbrown::HashMap;
use plank_core::{Idx, RelSlice};
use plank_session::{MaybePoisoned, Poisoned, Session, SourceId, SourceSpan, SrcLoc, StrId};
use plank_values::{Compound, FieldIdx, Type, TypeId, Value, ValueId, ValueInterner};

use crate::{
    diagnostics::{DiagCtx, InterfaceMethod},
    evaluator::Evaluator,
    scope::{Diverge, Scope},
};

#[derive(Clone, Copy)]
pub(crate) enum Implementation<I: StdInterface> {
    Valid(I),
    Missing,
    WrongReturnType(TypeId),
    Invalid(I::DecodeError),
    Poisoned,
}

pub(crate) struct StdInterfaces {
    impl_name: StrId,

    // `AsPrimitive`
    as_primitive: Option<MaybePoisoned<AsPrimitiveDef>>,
    as_primitive_implementations: HashMap<TypeId, Implementation<AsPrimitive>>,
}

impl StdInterfaces {
    pub(crate) fn new(session: &mut Session) -> Self {
        Self {
            impl_name: session.intern("impl"),

            as_primitive: None,
            as_primitive_implementations: HashMap::new(),
        }
    }

    pub(crate) fn resolve<'a>(
        core_interfaces_source: SourceId,
        evaluator: &mut Evaluator<'a>,
        diag_ctx: &mut DiagCtx<'a>,
    ) -> Self {
        Self {
            as_primitive: Some(resolve_as_primitive(core_interfaces_source, evaluator, diag_ctx)),
            ..Self::new(diag_ctx.session)
        }
    }

    pub(crate) fn as_primitive(&self) -> Option<MaybePoisoned<AsPrimitiveDef>> {
        self.as_primitive
    }
}

pub(crate) trait StdInterface: Copy {
    type Def: Copy;
    type DecodeError: Copy;

    fn interface_type(def: Self::Def) -> TypeId;

    fn implementations(
        interfaces: &mut StdInterfaces,
    ) -> &mut HashMap<TypeId, Implementation<Self>>;

    fn decode(
        values: &ValueInterner,
        def: Self::Def,
        impl_value: ValueId,
    ) -> Result<Self, Self::DecodeError>;

    fn emit_decode_error(diag_ctx: &mut DiagCtx<'_>, error: Self::DecodeError, loc: SrcLoc);
}

impl Scope<'_, '_> {
    pub(crate) fn eval_as_primitive(
        &mut self,
        def: AsPrimitiveDef,
        value: ValueId,
        span: SourceSpan,
    ) -> MaybePoisoned<Result<(U256, u8), Diverge>> {
        let as_primitive = match self.implementation::<AsPrimitive>(
            def,
            self.values.type_of_value(value),
            span,
        )? {
            Ok(as_primitive) => as_primitive,
            Err(diverge) => return Ok(Err(diverge)),
        };
        let to_raw_result =
            match self.eval_synthetic_comptime_call(as_primitive.to_raw, &[value], span)? {
                Ok(result) => result,
                Err(diverge) => return Ok(Err(diverge)),
            };
        let Value::BigNum(raw) = self.values.lookup(to_raw_result) else {
            self.diag_ctx.emit_interface_return_type_mismatch(
                self.eval.values,
                def.ty,
                InterfaceMethod::ToRaw,
                self.values.type_of_value(to_raw_result),
                self.loc(span),
            );
            return Err(Poisoned);
        };
        if raw.bit_len() > usize::from(as_primitive.byte_size) * 8 {
            self.diag_ctx.emit_as_primitive_raw_exceeds_byte_size(
                raw,
                as_primitive.byte_size,
                self.loc(span),
            );
            return Err(Poisoned);
        }
        Ok(Ok((raw, as_primitive.byte_size)))
    }

    pub(crate) fn implementation<I: StdInterface>(
        &mut self,
        def: I::Def,
        ty: TypeId,
        span: SourceSpan,
    ) -> MaybePoisoned<Result<I, Diverge>> {
        let implementation = match I::implementations(&mut self.eval.std_interfaces).get(&ty) {
            Some(&implementation) => implementation,
            None => match self.compute_implementation(def, ty, span) {
                Ok(implementation) => implementation,
                Err(diverge) => return Ok(Err(diverge)),
            },
        };
        let interface = I::interface_type(def);
        match implementation {
            Implementation::Valid(decoded) => Ok(Ok(decoded)),
            Implementation::Missing => {
                self.diag_ctx.emit_interface_not_implemented(
                    self.eval.values,
                    ty,
                    interface,
                    self.loc(span),
                );
                Err(Poisoned)
            }
            Implementation::WrongReturnType(actual) => {
                self.diag_ctx.emit_interface_return_type_mismatch(
                    self.eval.values,
                    interface,
                    InterfaceMethod::Impl,
                    actual,
                    self.loc(span),
                );
                Err(Poisoned)
            }
            Implementation::Invalid(error) => {
                let loc = self.loc(span);
                I::emit_decode_error(self.diag_ctx, error, loc);
                Err(Poisoned)
            }
            Implementation::Poisoned => Err(Poisoned),
        }
    }

    fn compute_implementation<I: StdInterface>(
        &mut self,
        def: I::Def,
        ty: TypeId,
        span: SourceSpan,
    ) -> Result<Implementation<I>, Diverge> {
        let interface = I::interface_type(def);
        let method = match self.types.lookup(ty) {
            Type::Compound(Compound::Struct(r#struct)) => {
                self.find_method(r#struct, self.eval.std_interfaces.impl_name)
            }
            _ => None,
        };
        let implementation = if let Some(method) = method {
            let closure = self.bind_method_self(method, ty);
            let argument = self.eval.values.intern_type(interface);
            match self.eval_synthetic_comptime_call(closure, &[argument], span) {
                Ok(Ok(value)) => match self.values.type_of_value(value) {
                    TypeId::VOID => Implementation::Missing,
                    actual if actual == interface => {
                        match I::decode(self.eval.values, def, value) {
                            Ok(decoded) => Implementation::Valid(decoded),
                            Err(error) => Implementation::Invalid(error),
                        }
                    }
                    actual => Implementation::WrongReturnType(actual),
                },
                Ok(Err(diverge)) => return Err(diverge),
                Err(Poisoned) => Implementation::Poisoned,
            }
        } else {
            Implementation::Missing
        };
        I::implementations(&mut self.eval.std_interfaces).insert(ty, implementation);
        Ok(implementation)
    }
}

#[derive(Clone, Copy)]
struct AsPrimitive {
    byte_size: u8,
    to_raw: ValueId,
}

impl StdInterface for AsPrimitive {
    type Def = AsPrimitiveDef;
    type DecodeError = U256;

    fn interface_type(def: AsPrimitiveDef) -> TypeId {
        def.ty
    }

    fn implementations(
        interfaces: &mut StdInterfaces,
    ) -> &mut HashMap<TypeId, Implementation<Self>> {
        &mut interfaces.as_primitive_implementations
    }

    fn decode(
        values: &ValueInterner,
        def: AsPrimitiveDef,
        impl_value: ValueId,
    ) -> Result<Self, U256> {
        let byte_size_value = interface_member(values, impl_value, def.byte_size);
        let to_raw = interface_member(values, impl_value, def.to_raw);
        let Value::BigNum(byte_size) = values.lookup(byte_size_value) else {
            unreachable!("invariant: interface definition validated byte_size as u256")
        };
        match u8::try_from(byte_size) {
            Ok(byte_size) if byte_size <= 32 => Ok(Self { byte_size, to_raw }),
            _ => Err(byte_size),
        }
    }

    fn emit_decode_error(diag_ctx: &mut DiagCtx<'_>, byte_size: U256, loc: SrcLoc) {
        diag_ctx.emit_invalid_as_primitive_byte_size(byte_size, loc);
    }
}

#[derive(Clone, Copy)]
pub(crate) struct AsPrimitiveDef {
    ty: TypeId,
    byte_size: FieldIdx,
    to_raw: FieldIdx,
}

fn resolve_as_primitive<'a>(
    source: SourceId,
    evaluator: &mut Evaluator<'a>,
    diag_ctx: &mut DiagCtx<'a>,
) -> MaybePoisoned<AsPrimitiveDef> {
    let session = &mut *diag_ctx.session;
    let name = session.intern("AsPrimitive");
    let required_members = [
        (session.intern("byte_size"), TypeId::U256),
        (session.intern("to_raw"), TypeId::FUNCTION),
        (session.intern("unchecked_from_raw"), TypeId::FUNCTION),
    ];
    let ty = resolve_std_interface(source, name, evaluator, diag_ctx)?;
    let [byte_size, to_raw, _unchecked_from_raw] =
        validate_interface_definition(ty, required_members, evaluator, diag_ctx)?;
    Ok(AsPrimitiveDef { ty, byte_size, to_raw })
}

fn resolve_std_interface<'a>(
    source: SourceId,
    name: StrId,
    evaluator: &mut Evaluator<'a>,
    diag_ctx: &mut DiagCtx<'a>,
) -> MaybePoisoned<TypeId> {
    let hir = evaluator.hir;
    let Some(const_id) = hir.find_const(source, name) else {
        diag_ctx.emit_failed_to_resolve_std_interface(source, name);
        return Err(Poisoned);
    };
    let value = evaluator.evaluate_const(const_id, diag_ctx)?;
    if let Value::Type(ty) = evaluator.values.lookup(value)
        && ty.is_struct()
    {
        return Ok(ty);
    }
    diag_ctx.emit_std_interface_not_a_struct(name, hir.consts[const_id].loc());
    Err(Poisoned)
}

fn validate_interface_definition<const N: usize>(
    interface: TypeId,
    required_members: [(StrId, TypeId); N],
    evaluator: &Evaluator<'_>,
    diag_ctx: &mut DiagCtx<'_>,
) -> MaybePoisoned<[FieldIdx; N]> {
    let Type::Compound(Compound::Struct(r#struct)) = evaluator.types.lookup(interface) else {
        unreachable!("invariant: interface was checked to be a struct type")
    };
    let fields = RelSlice::<FieldIdx, _>::new(FieldIdx::ZERO, r#struct.fields);
    let mut invalid = false;
    let indices = required_members.map(|(name, expected)| {
        let found = fields.enumerate_idx().find(|(_, field)| field.name == name);
        let valid = found.filter(|(_, field)| field.ty == expected);
        if valid.is_none() {
            let loc = found.map_or(r#struct.def_loc, |(_, field)| {
                SrcLoc::new(r#struct.def_loc.source, field.def_span)
            });
            diag_ctx.emit_invalid_interface_definition_field(
                evaluator.values,
                interface,
                name,
                expected,
                found.map(|(_, field)| field.ty),
                loc,
            );
            invalid = true;
        }
        valid.map(|(index, _)| index)
    });
    if invalid {
        return Err(Poisoned);
    }
    Ok(indices.map(|index| index.expect("invariant: invalid members were reported above")))
}

fn interface_member(values: &ValueInterner, impl_value: ValueId, index: FieldIdx) -> ValueId {
    let Value::Compound { fields, .. } = values.lookup(impl_value) else {
        unreachable!("invariant: interface implementation was checked to be a struct")
    };
    RelSlice::new(FieldIdx::ZERO, fields)[index]
}
