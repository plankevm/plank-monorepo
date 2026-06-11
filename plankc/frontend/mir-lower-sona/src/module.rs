use crate::{
    CONTRACT_OBJECT, INIT_SECTION, LowerError, RUNTIME_SECTION, builder_error,
    function::{FunctionLowerer, LoweringContext},
};
use plank_core::{DenseIndexMap, Idx};
use plank_mir::{Expr, Instruction, Mir};
use plank_session::{BytesId, Session};
use plank_values::{PrimitiveType, Type as PlankType, TypeId, ValueInterner};
use sonatina_ir::{
    GlobalVariableRef, Linkage, Module, Signature, Type as SonaType,
    builder::{ModuleBuilder, ObjectBuilder},
    global_variable::{GlobalVariableData, GvInitializer},
    isa::{Isa, evm::Evm},
    module::{FuncRef, ModuleCtx},
};
use std::collections::HashMap;

pub(crate) type RuntimeShapes = HashMap<TypeId, Option<SonaType>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SectionContext {
    Init,
    Runtime,
}

struct SectionReachability {
    init: DenseIndexSet<plank_mir::FnId>,
    runtime: DenseIndexSet<plank_mir::FnId>,
}

pub(crate) type DataGlobals = HashMap<BytesId, GlobalVariableRef>;

pub(crate) fn runtime_shape(shapes: &RuntimeShapes, ty: TypeId) -> Option<SonaType> {
    *shapes.get(&ty).expect("type was not declared before lowering")
}

fn declare_runtime_shape(
    shapes: &mut RuntimeShapes,
    mir: &Mir,
    builder: &ModuleBuilder,
    ty: TypeId,
) -> Option<SonaType> {
    if let Some(&shape) = shapes.get(&ty) {
        return shape;
    }
    let shape = match mir.types.lookup(ty) {
        PlankType::Primitive(primitive) => match primitive {
            PrimitiveType::Void | PrimitiveType::Never => None,
            PrimitiveType::Bool => Some(SonaType::I1),
            PrimitiveType::U256 | PrimitiveType::MemoryPointer => Some(SonaType::I256),
            PrimitiveType::Function | PrimitiveType::Type | PrimitiveType::CBytes => {
                panic!("comptime-only type in MIR: {primitive:?}")
            }
        },
        PlankType::Struct(struct_) => {
            let field_shapes = struct_
                .fields
                .iter()
                .map(|field| declare_runtime_shape(shapes, mir, builder, field.ty))
                .collect::<Vec<_>>();
            if field_shapes.iter().all(Option::is_none) {
                None
            } else {
                let field_tys =
                    field_shapes.iter().map(|s| s.unwrap_or(SonaType::Unit)).collect::<Vec<_>>();
                Some(builder.declare_struct_type(
                    &format!("struct_{}", ty.get()),
                    &field_tys,
                    false,
                ))
            }
        }
    };
    shapes.insert(ty, shape);
    shape
}

/// Declares one constant global per unique interned bytes value referenced by
/// a `DataOffset` expression. Codegen places each global's bytes in the data
/// section of the code, making its symbol address valid for `codecopy`.
fn declare_data_globals(builder: &ModuleBuilder, mir: &Mir, session: &Session) -> DataGlobals {
    let mut globals = DataGlobals::new();
    for block in mir.blocks.iter() {
        for instr in block {
            let Instruction::Set { expr: Expr::DataOffset { contents, .. }, .. } = *instr else {
                continue;
            };
            let next_idx = globals.len();
            globals.entry(contents).or_insert_with(|| {
                let bytes = session.lookup_bytes(contents);
                let ty = builder.declare_array_type(SonaType::I8, bytes.len());
                let initializer = GvInitializer::make_array(
                    bytes.iter().map(|&byte| GvInitializer::make_imm(byte)).collect(),
                );
                builder.declare_gv(GlobalVariableData::constant(
                    format!("cbytes_{next_idx}"),
                    ty,
                    Linkage::Private,
                    initializer,
                ))
            });
        }
    }
    globals
}

pub(crate) fn lower(
    isa: &Evm,
    mir: &Mir,
    values: &ValueInterner,
    session: &Session,
) -> Result<Module, LowerError> {
    let is = isa.inst_set();
    let mut builder = ModuleBuilder::new(ModuleCtx::new(isa));
    let mut runtime_shapes = RuntimeShapes::new();
    let mut funcs = DenseIndexMap::with_capacity(mir.fns.len());
    let data_globals = declare_data_globals(&builder, mir, session);

    // Declare runtime shapes before function signatures so aggregate type refs exist.
    for fn_id in mir.fns.iter_idx() {
        for &ty in &mir.fn_locals[fn_id] {
            declare_runtime_shape(&mut runtime_shapes, mir, &builder, ty);
        }
        declare_runtime_shape(&mut runtime_shapes, mir, &builder, mir.fns[fn_id].return_type);
    }

    for fn_id in mir.fns.iter_idx() {
        if reachability.init.contains(fn_id) {
            let func = declare_function(
                &builder,
                mir,
                &runtime_shapes,
                &reachability,
                fn_id,
                SectionContext::Init,
            )?;
            init_funcs.insert_no_prev(fn_id, func);
        }
        if reachability.runtime.contains(fn_id) {
            let func = declare_function(
                &builder,
                mir,
                &runtime_shapes,
                &reachability,
                fn_id,
                SectionContext::Runtime,
            )?;
            runtime_funcs.insert_no_prev(fn_id, func);
        }
    }

    for fn_id in mir.fns.iter_idx() {
        FunctionLowerer::new(
            &builder,
            is,
            mir,
            values,
            &funcs,
            &runtime_shapes,
            &data_globals,
            fn_id,
        )
        .lower();
    }

    // Build the EVM object last so init can embed the completed runtime section.
    let mut object = ObjectBuilder::new(CONTRACT_OBJECT);
    if let Some(run) = mir.run {
        object.section(RUNTIME_SECTION).entry(runtime_funcs[run]);
    }
    let init = object.section(INIT_SECTION).entry(init_funcs[mir.init]);
    if mir.run.is_some() {
        init.embed_local(RUNTIME_SECTION, RUNTIME_SECTION);
    }
    object.declare(&mut builder).map_err(builder_error)?;
    Ok(builder.build())
}
