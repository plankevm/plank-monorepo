use crate::{
    CONTRACT_OBJECT, INIT_SECTION, LowerError, RUNTIME_SECTION, builder_error,
    function::FunctionLowerer,
};
use plank_core::{DenseIndexMap, Idx};
use plank_mir::Mir;
use plank_values::{PrimitiveType, Type as PlankType, TypeId, ValueInterner};
use sonatina_ir::{
    Linkage, Module, Signature, Type as SonaType,
    builder::{ModuleBuilder, ObjectBuilder},
    isa::{Isa, evm::Evm},
    module::ModuleCtx,
};
use std::collections::HashMap;

pub(crate) type RuntimeShapes = HashMap<TypeId, Option<SonaType>>;

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
            PrimitiveType::Function | PrimitiveType::Type => {
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

pub(crate) fn lower(isa: &Evm, mir: &Mir, values: &ValueInterner) -> Result<Module, LowerError> {
    let is = isa.inst_set();
    let mut builder = ModuleBuilder::new(ModuleCtx::new(isa));
    let mut runtime_shapes = RuntimeShapes::new();
    let mut funcs = DenseIndexMap::with_capacity(mir.fns.len());

    // Declare runtime shapes before function signatures so aggregate type refs exist.
    for fn_id in mir.fns.iter_idx() {
        for &ty in &mir.fn_locals[fn_id] {
            declare_runtime_shape(&mut runtime_shapes, mir, &builder, ty);
        }
        declare_runtime_shape(&mut runtime_shapes, mir, &builder, mir.fns[fn_id].return_type);
    }

    // Declare all functions before bodies so calls can reference any MIR function.
    for fn_id in mir.fns.iter_idx() {
        let def = mir.fns[fn_id];
        let mut args = Vec::new();
        for param in def.iter_params() {
            args.extend(runtime_shape(&runtime_shapes, mir.fn_locals[fn_id][param.idx()]));
        }
        let returns: Vec<_> = runtime_shape(&runtime_shapes, def.return_type).into_iter().collect();

        let (name, linkage) = if fn_id == mir.init {
            ("init".to_string(), Linkage::Public)
        } else if Some(fn_id) == mir.run {
            ("run".to_string(), Linkage::Public)
        } else {
            (format!("fn_{}", fn_id.idx()), Linkage::Private)
        };

        let func = builder
            .declare_function(Signature::new(&name, linkage, &args, &returns))
            .map_err(builder_error)?;
        funcs.insert(fn_id, func);
    }

    // Lower bodies after declarations so recursive and forward calls are valid.
    for fn_id in mir.fns.iter_idx() {
        FunctionLowerer::new(&builder, is, mir, values, &funcs, &runtime_shapes, fn_id).lower();
    }

    // Build the EVM object last so init can embed the completed runtime section.
    let mut object = ObjectBuilder::new(CONTRACT_OBJECT);
    if let Some(run) = mir.run {
        object.section(RUNTIME_SECTION).entry(funcs[run]);
    }
    let init = object.section(INIT_SECTION).entry(funcs[mir.init]);
    if mir.run.is_some() {
        init.embed_local(RUNTIME_SECTION, RUNTIME_SECTION);
    }
    object.declare(&mut builder).map_err(builder_error)?;
    Ok(builder.build())
}
