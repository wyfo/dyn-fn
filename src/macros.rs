macro_rules! impl_debug {
    (async $name:ident $(.$field:tt)?, $fn_storage:ident ) => {
        $crate::macros::impl_debug!(@ $name $(.$field)?, $fn_storage, FutureStorage: StorageMut);
    };
    (sync $name:ident $(.$field:tt)?, $fn_storage:ident) => {
        $crate::macros::impl_debug!(@ $name $(.$field)?, $fn_storage);
    };
    (@ $name:ident $(.$field:tt)?, $fn_storage:ident $(, $future_storage:ident: $future_storage_bound:ident)?) => {
        impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: $fn_storage, $($future_storage: $future_storage_bound)?> core::fmt::Debug
            for $name<'capture, Arg, Ret, FnStorage, $($future_storage)?>
        {
            #[cfg_attr(coverage_nightly, coverage(off))]
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.debug_struct(stringify!($name))
                    .field("func", &self$(.$field)?.func)
                    .field("call", &self$(.$field)?.call)
                    .finish()
            }
        }

    };
}
pub(crate) use impl_debug;

macro_rules! unsafe_impl_send_sync {
    (async $name:ident, $fn_storage:ident ) => {
        $crate::macros::unsafe_impl_send_sync!(@ $name, $fn_storage, FutureStorage: StorageMut);
    };
    (sync $name:ident, $fn_storage:ident) => {
        $crate::macros::unsafe_impl_send_sync!(@ $name, $fn_storage);
    };
    (@ $name:ident, $fn_storage:ident $(, $future_storage:ident: $future_storage_bound:ident)?) => {
        $crate::macros::unsafe_impl_send_sync!(@ Send: $name, $fn_storage $(, $future_storage: $future_storage_bound)?);
        $crate::macros::unsafe_impl_send_sync!(@ Sync: $name, $fn_storage $(, $future_storage: $future_storage_bound)?);
    };
    (@ $trait:ident: $name:ident $(.$field:tt)?, $fn_storage:ident $(, $future_storage:ident: $future_storage_bound:ident)?) => {
        // SAFETY: the object is initialized with a `Send + Sync` function
        unsafe impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: $fn_storage, $($future_storage: $future_storage_bound)?> $trait
            for $name<'capture, Arg, Ret, FnStorage, $($future_storage)?>
        {
        }
        };
}
pub(crate) use unsafe_impl_send_sync;
