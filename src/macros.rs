macro_rules! new_impls {
    (sync $(($arc:ident))? $name:ident, $storage:ident, $fn_storage:ident, $($f:tt)*) => {
        crate::macros::new_impls!(@ $(($arc))? $name, $storage, $fn_storage, {$($f)*}, new_impl, new, new_raw, new_box, new_arc);
    };
    (async $(($arc:ident))? $name:ident, $storage:ident, $fn_storage:ident, [$($f_sync:tt)*], $($f:tt)*) => {
        crate::macros::new_impls!(@ $(($arc))? $name, $storage, $fn_storage, {$($f)*}, new_impl, new, new_raw, new_box, new_arc, FutureStorage);
        crate::macros::new_impls!(@ $(($arc))? $name, $storage, $fn_storage, {$($f_sync)*}, new_sync_impl, new_sync, new_sync_raw, new_sync_box, new_sync_arc, FutureStorage);
    };
    (@ $name:ident, $storage:ident, $fn_storage:ident, {$($f:tt)*}, $new_impl:ident, $new:ident, $new_raw:ident, $new_box:ident, $new_arc:ident $(, $future_storage:ident)?) => {
        impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: $fn_storage, $($future_storage: StorageMut)?>
            $name<'capture, Arg, Ret, FnStorage, $($future_storage)?>
        {
            pub fn $new<F: $($f)*>(
                f: F,
            ) -> Self {
                Self::$new_impl::<F>($storage::new(f))
            }
        }

        impl<'capture, Arg: ForLt, Ret: ForLt, const SIZE: usize, const ALIGN: usize, $($future_storage: StorageMut)?>
            $name<'capture, Arg, Ret, crate::storage::Raw<SIZE, ALIGN>, $($future_storage)?>
        where
            elain::Align<ALIGN>: elain::Alignment,
        {
            #[cfg_attr(coverage_nightly, coverage(off))]
            pub const fn $new_raw<F: $($f)*>(
                f: F,
            ) -> Self {
                Self::$new_impl::<F>($storage::new_raw(f))
            }
        }

        #[cfg(feature = "alloc")]
        impl<'capture, Arg: ForLt, Ret: ForLt, $($future_storage: StorageMut)?> $name<'capture, Arg, Ret, crate::storage::Box, $($future_storage)?> {
            #[cfg_attr(coverage_nightly, coverage(off))]
            pub fn $new_box<F: $($f)*>(
                f: alloc::boxed::Box<F>,
            ) -> Self {
                Self::$new_impl::<F>($storage::new_box(f))
            }
        }

        #[cfg(feature = "alloc")]
        impl<'capture, Arg: ForLt, Ret: ForLt, const SIZE: usize, const ALIGN: usize, $($future_storage: StorageMut)?>
            $name<'capture, Arg, Ret, crate::storage::RawOrBox<SIZE, ALIGN>, $($future_storage)?>
        where
            elain::Align<ALIGN>: elain::Alignment,
        {
            #[cfg_attr(coverage_nightly, coverage(off))]
            pub const fn $new_raw<F: $($f)*>(
                f: F,
            ) -> Self {
                Self::$new_impl::<F>($storage::new_raw2(f))
            }

            #[cfg_attr(coverage_nightly, coverage(off))]
            #[cfg(feature = "alloc")]
            pub fn $new_box<F: $($f)*>(
                f: alloc::boxed::Box<F>,
            ) -> Self {
                Self::$new_impl::<F>($storage::new_box2(f))
            }
        }


    };
    (@(arc) $name:ident, $storage:ident, $fn_storage:ident, {$($f:tt)*}, $new_impl:ident, $new:ident, $new_raw:ident, $new_box:ident, $new_arc:ident $(, $future_storage:ident)?) => {
        crate::macros::new_impls!(@ $name, $storage, $fn_storage, {$($f)*}, $new_impl, $new, $new_raw, $new_box, $new_arc $(, $future_storage)?);
        #[cfg(feature = "alloc")]
        impl<'capture, Arg: ForLt, Ret: ForLt, $($future_storage: StorageMut)?> $name<'capture, Arg, Ret, crate::storage::Arc, $($future_storage)?> {
            #[cfg_attr(coverage_nightly, coverage(off))]
            pub fn $new_arc<F: $($f)*>(
                f: alloc::sync::Arc<F>,
            ) -> Self {
                Self::$new_impl::<F>($storage::new_arc(f))
            }
        }
    }
}
pub(crate) use new_impls;

macro_rules! unsafe_impl_send_sync {
    (async $name:ident, $fn_storage:ident ) => {
        crate::macros::unsafe_impl_send_sync!(@ $name, $fn_storage, FutureStorage);
    };
    (sync $name:ident, $fn_storage:ident) => {
        crate::macros::unsafe_impl_send_sync!(@ $name, $fn_storage);
    };
    (@ $name:ident, $fn_storage:ident $(, $future_storage:ident)?) => {
        crate::macros::unsafe_impl_send_sync!(@ Send: $name, $fn_storage $(, $future_storage)?);
        crate::macros::unsafe_impl_send_sync!(@ Sync: $name, $fn_storage $(, $future_storage)?);
    };
    (@ $trait:ident: $name:ident $(.$field:tt)?, $fn_storage:ident $(, $future_storage:ident)?) => {
        // SAFETY: the object is initialized with a `Send + Sync` function
        unsafe impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: $fn_storage, $($future_storage: StorageMut)?> $trait
            for $name<'capture, Arg, Ret, FnStorage, $($future_storage)?>
        {
        }
        };
}
pub(crate) use unsafe_impl_send_sync;

macro_rules! impl_debug {
    (async $name:ident $(.$field:tt)?, $fn_storage:ident ) => {
        crate::macros::impl_debug!(@ $name $(.$field)?, $fn_storage, FutureStorage);
    };
    (sync $name:ident $(.$field:tt)?, $fn_storage:ident) => {
        crate::macros::impl_debug!(@ $name $(.$field)?, $fn_storage);
    };
    (@ $name:ident $(.$field:tt)?, $fn_storage:ident $(, $future_storage:ident)?) => {
        impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: $fn_storage, $($future_storage: StorageMut)?> core::fmt::Debug
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
