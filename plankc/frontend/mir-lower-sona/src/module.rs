use crate::{
    CONTRACT_OBJECT, INIT_SECTION, LowerError, RUNTIME_SECTION, builder_error,
    function::{FunctionLowerer, LoweringContext},
};
use plank_core::{DenseIndexMap, DenseIndexSet, Idx};
use plank_mir::{self as mir, Expr, Instruction, Mir};
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

impl SectionReachability {
    fn new(mir: &Mir) -> Self {
        let mut init = DenseIndexSet::with_capacity_in_bits(mir.fns.len());
        collect_reachable_fns(mir, mir.init, &mut init);

        let mut runtime = DenseIndexSet::with_capacity_in_bits(mir.fns.len());
        if let Some(run) = mir.run {
            collect_reachable_fns(mir, run, &mut runtime);
        }

        Self { init, runtime }
    }
}

fn collect_reachable_fns(mir: &Mir, root: mir::FnId, reachable: &mut DenseIndexSet<mir::FnId>) {
    let mut fn_worklist = Vec::with_capacity(mir.fns.len());
    let mut block_seen = DenseIndexSet::with_capacity_in_bits(mir.blocks.len());
    if reachable.add(root) {
        fn_worklist.push(root);
    }

    while let Some(fn_id) = fn_worklist.pop() {
        block_seen.clear();
        collect_block_callees(
            mir,
            mir.fns[fn_id].body,
            &mut block_seen,
            reachable,
            &mut fn_worklist,
        );
    }
}

fn collect_block_callees(
    mir: &Mir,
    block: mir::BlockId,
    block_seen: &mut DenseIndexSet<mir::BlockId>,
    reachable: &mut DenseIndexSet<mir::FnId>,
    fn_worklist: &mut Vec<mir::FnId>,
) {
    if !block_seen.add(block) {
        return;
    }

    for &instr in &mir.blocks[block] {
        match instr {
            Instruction::Set { expr: Expr::Call { callee, .. }, .. } => {
                if reachable.add(callee) {
                    fn_worklist.push(callee);
                }
            }
            Instruction::Set { .. } | Instruction::Return(_) => {}
            Instruction::If { then_block, else_block, .. } => {
                collect_block_callees(mir, then_block, block_seen, reachable, fn_worklist);
                collect_block_callees(mir, else_block, block_seen, reachable, fn_worklist);
            }
            Instruction::While { condition_block, body, .. } => {
                collect_block_callees(mir, condition_block, block_seen, reachable, fn_worklist);
                collect_block_callees(mir, body, block_seen, reachable, fn_worklist);
            }
        }
    }
}

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

fn declare_function(
    builder: &ModuleBuilder,
    mir: &Mir,
    runtime_shapes: &RuntimeShapes,
    reachability: &SectionReachability,
    fn_id: mir::FnId,
    context: SectionContext,
) -> Result<FuncRef, LowerError> {
    let def = mir.fns[fn_id];
    let mut args = Vec::new();
    for param in def.iter_params() {
        args.extend(runtime_shape(runtime_shapes, mir.fn_locals[fn_id][param.idx()]));
    }
    let returns: Vec<_> = runtime_shape(runtime_shapes, def.return_type).into_iter().collect();
    let name = function_name(mir, reachability, fn_id, context);
    let linkage = if fn_id == mir.init || Some(fn_id) == mir.run {
        Linkage::Public
    } else {
        Linkage::Private
    };

    builder.declare_function(Signature::new(&name, linkage, &args, &returns)).map_err(builder_error)
}

fn function_name(
    mir: &Mir,
    reachability: &SectionReachability,
    fn_id: mir::FnId,
    context: SectionContext,
) -> String {
    if fn_id == mir.init {
        return "init".to_string();
    }
    if Some(fn_id) == mir.run {
        return "run".to_string();
    }
    if context == SectionContext::Runtime && reachability.init.contains(fn_id) {
        return format!("fn_{}_runtime", fn_id.idx());
    }
    format!("fn_{}", fn_id.idx())
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
    let reachability = SectionReachability::new(mir);
    let mut init_funcs = DenseIndexMap::with_capacity(mir.fns.len());
    let mut runtime_funcs = DenseIndexMap::with_capacity(mir.fns.len());
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
        if init_funcs.contains(fn_id) {
            FunctionLowerer::new(
                &builder,
                is,
                mir,
                values,
                &data_globals,
                fn_id,
                LoweringContext {
                    funcs: &init_funcs,
                    runtime_shapes: &runtime_shapes,
                    section_context: SectionContext::Init,
                },
            )
            .lower();
        }
        if runtime_funcs.contains(fn_id) {
            FunctionLowerer::new(
                &builder,
                is,
                mir,
                values,
                &data_globals,
                fn_id,
                LoweringContext {
                    funcs: &runtime_funcs,
                    runtime_shapes: &runtime_shapes,
                    section_context: SectionContext::Runtime,
                },
            )
            .lower();
        }
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
