macro_rules! with_buf_methods {
    ($($name:ident => $buf:ident;)*) => {
        $(
            pub fn $name<R>(&mut self, inner: impl FnOnce(&mut Self, usize) -> R) -> R {
                let buf_offset = self.$buf.len();
                let res = inner(self, buf_offset);
                self.$buf.truncate(buf_offset);
                res
            }
        )*
    };
}

impl crate::scope::Scope<'_, '_> {
    with_buf_methods! {
        with_values_buf => values_buf;
        with_types_buf => types_buf;
        with_fields_buf => fields_buf;
        with_locals_buf => locals_buf;
        with_captures_buf => captures_buf;
    }
}
