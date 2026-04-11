use plank_core::newtype_index;

newtype_index! {
    pub struct TypeId;
}

impl TypeId {
    pub const VOID: TypeId = TypeId::new(0);
    pub const U256: TypeId = TypeId::new(1);
    pub const BOOL: TypeId = TypeId::new(2);
    pub const MEMORY_POINTER: TypeId = TypeId::new(3);
    pub const TYPE: TypeId = TypeId::new(4);
    pub const FUNCTION: TypeId = TypeId::new(5);
    pub const NEVER: TypeId = TypeId::new(6);

    pub const LAST_FIXED_ID: TypeId = Self::NEVER;
    pub const STRUCT_IDS_OFFSET: u32 = Self::LAST_FIXED_ID.const_get() + 1;

    pub const fn is_struct(self) -> bool {
        self.const_get() > Self::LAST_FIXED_ID.const_get()
    }

    pub fn is_assignable_to(self, target: TypeId) -> bool {
        self == target || self == TypeId::NEVER
    }

    pub fn unify(&mut self, other: TypeId) -> Result<(), TypeId> {
        if *self == TypeId::NEVER {
            *self = other;
            return Ok(());
        }
        if other == TypeId::NEVER || *self == other {
            return Ok(());
        }
        Err(*self)
    }
}
