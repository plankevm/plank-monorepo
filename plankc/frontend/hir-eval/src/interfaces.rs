use alloy_primitives::U256;
use hashbrown::HashMap;
use plank_core::{Idx, RelSlice};
use plank_session::{MaybePoisoned, Poisoned, Session, SourceId, SourceSpan, SrcLoc, StrId};
use plank_values::{Compound, Type, TypeId, Value, ValueId, ValueIdx, ValueInterner};

use crate::{
    diagnostics::DiagCtx,
    evaluator::Evaluator,
    scope::{Diverge, Scope},
};

#[derive(Clone, Copy)]
enum CachedInterface {
    Implemented(ValueId),
    NotImplemented,
    InvalidReturnType(TypeId),
    Poisoned,
}

#[derive(Clone, Copy)]
pub(crate) struct InterfaceDefinition<M> {
    ty: TypeId,
    members: M,
}

#[derive(Clone, Copy)]
pub(crate) struct AsPrimitiveMembers {
    byte_size: ValueIdx,
    to_raw: ValueIdx,
}

pub(crate) struct InterfaceCache {
    implementations: HashMap<(TypeId, TypeId), CachedInterface>,
    as_primitive: Option<MaybePoisoned<InterfaceDefinition<AsPrimitiveMembers>>>,

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
            implementations: HashMap::new(),
            as_primitive: None,

            impl_name: session.intern("impl"),

            as_primitive_name: session.intern("AsPrimitive"),
            byte_size_name: session.intern("byte_size"),
            to_raw_name: session.intern("to_raw"),
            unchecked_from_raw_name: session.intern("unchecked_from_raw"),
        }
    }

    pub(crate) fn with_std_interfaces<'a>(
        core_interfaces_source: SourceId,
        evaluator: &mut Evaluator<'a>,
        diag_ctx: &mut DiagCtx<'a>,
    ) -> Self {
        let mut cache = Self::new(diag_ctx.session);
        cache.as_primitive =
            Some(resolve_as_primitive(&cache, core_interfaces_source, evaluator, diag_ctx));
        cache
    }

    pub(crate) fn as_primitive(
        &self,
    ) -> Option<MaybePoisoned<InterfaceDefinition<AsPrimitiveMembers>>> {
        self.as_primitive
    }
}

pub(crate) trait CompilerInterface: Sized {
    type Members: Copy;

    fn decode(
        scope: &mut Scope<'_, '_>,
        definition: InterfaceDefinition<Self::Members>,
        implementation: ValueId,
        span: SourceSpan,
    ) -> MaybePoisoned<Self>;
}

struct AsPrimitive {
    byte_size: u8,
    to_raw: ValueId,
}

impl CompilerInterface for AsPrimitive {
    type Members = AsPrimitiveMembers;

    fn decode(
        scope: &mut Scope<'_, '_>,
        definition: InterfaceDefinition<AsPrimitiveMembers>,
        implementation: ValueId,
        span: SourceSpan,
    ) -> MaybePoisoned<Self> {
        let byte_size_value =
            interface_member(scope.eval.values, implementation, definition.members.byte_size);
        let to_raw = interface_member(scope.eval.values, implementation, definition.members.to_raw);
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
        Ok(Self { byte_size, to_raw })
    }
}

impl Scope<'_, '_> {
    pub(crate) fn eval_as_primitive(
        &mut self,
        definition: InterfaceDefinition<AsPrimitiveMembers>,
        value: ValueId,
        span: SourceSpan,
    ) -> MaybePoisoned<Result<(U256, u8), Diverge>> {
        let implementation = match self.eval_interface::<AsPrimitive>(
            definition,
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
                definition.ty,
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

    pub(crate) fn eval_interface<I: CompilerInterface>(
        &mut self,
        definition: InterfaceDefinition<I::Members>,
        ty: TypeId,
        span: SourceSpan,
    ) -> MaybePoisoned<Result<I, Diverge>> {
        let interface = definition.ty;
        let cached = match self.eval.interface_cache.implementations.get(&(interface, ty)) {
            Some(&cached) => cached,
            None => match self.cache_interface(interface, ty, span) {
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
            CachedInterface::Poisoned => return Err(Poisoned),
        };
        I::decode(self, definition, implementation, span).map(Ok)
    }

    fn cache_interface(
        &mut self,
        interface: TypeId,
        ty: TypeId,
        span: SourceSpan,
    ) -> Result<CachedInterface, Diverge> {
        let method = match self.types.lookup(ty) {
            Type::Compound(Compound::Struct(r#struct)) => {
                self.find_method(r#struct, self.eval.interface_cache.impl_name)
            }
            _ => None,
        };
        let implementation = if let Some(method) = method {
            let closure = self.bind_method_self(method, ty);
            let argument = self.eval.values.intern_type(interface);
            match self.eval_synthetic_call(closure, &[argument], span) {
                Ok(Ok(value)) => match self.values.type_of_value(value) {
                    TypeId::VOID => CachedInterface::NotImplemented,
                    actual if actual == interface => CachedInterface::Implemented(value),
                    actual => CachedInterface::InvalidReturnType(actual),
                },
                Ok(Err(diverge)) => return Err(diverge),
                Err(Poisoned) => CachedInterface::Poisoned,
            }
        } else {
            CachedInterface::NotImplemented
        };
        self.eval.interface_cache.implementations.insert((interface, ty), implementation);
        Ok(implementation)
    }
}

fn interface_member(values: &ValueInterner, implementation: ValueId, index: ValueIdx) -> ValueId {
    let Value::Compound { fields, .. } = values.lookup(implementation) else {
        unreachable!("invariant: interface implementation was checked to be a struct")
    };
    RelSlice::new(ValueIdx::ZERO, fields)[index]
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
) -> MaybePoisoned<[ValueIdx; N]> {
    let Type::Compound(Compound::Struct(r#struct)) = evaluator.types.lookup(interface) else {
        unreachable!("invariant: interface was checked to be a struct type")
    };
    let fields = RelSlice::<ValueIdx, _>::new(ValueIdx::ZERO, r#struct.fields);
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

fn resolve_as_primitive<'a>(
    cache: &InterfaceCache,
    source: SourceId,
    evaluator: &mut Evaluator<'a>,
    diag_ctx: &mut DiagCtx<'a>,
) -> MaybePoisoned<InterfaceDefinition<AsPrimitiveMembers>> {
    let name = cache.as_primitive_name;
    let required_members = [
        (cache.byte_size_name, TypeId::U256),
        (cache.to_raw_name, TypeId::FUNCTION),
        (cache.unchecked_from_raw_name, TypeId::FUNCTION),
    ];
    let interface = resolve_std_interface(source, name, evaluator, diag_ctx)?;
    let [byte_size, to_raw, _unchecked_from_raw] =
        validate_interface_definition(interface, required_members, evaluator, diag_ctx)?;
    Ok(InterfaceDefinition { ty: interface, members: AsPrimitiveMembers { byte_size, to_raw } })
}
