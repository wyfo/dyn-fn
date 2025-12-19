macro_rules! new_impls {
    (sync $name:ident, $fn_storage:ident $(+ $storage_send:ident)?, $($f:tt)*) => {
        crate::macros::new_impls!(@ $name, $fn_storage $(+ $storage_send)?, {$($f)*}, new_impl, new, new_box, new_rc, new_arc);
    };
    (async $name:ident, $fn_storage:ident $(+ $storage_send:ident)?, [$($f_sync:tt)*], $($f:tt)*) => {
        crate::macros::new_impls!(@ $name, $fn_storage $(+ $storage_send)?, {$($f)*}, new_impl, new,  new_box, new_rc, new_arc, FutureStorage);
        crate::macros::new_impls!(@ $name, $fn_storage $(+ $storage_send)?, {$($f_sync)*}, new_sync_impl, new_sync, new_sync_box, new_sync_rc, new_sync_arc, FutureStorage);
    };
    (@ $name:ident, $fn_storage:ident $(+ $storage_send:ident)?, {$($f:tt)*}, $new_impl:ident, $new:ident, $new_box:ident, $new_rc:ident, $new_arc:ident $(, $future_storage:ident)?) => {
        impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: $fn_storage $(+ $storage_send)?, $($future_storage: StorageMut)?>
            $name<'capture, Arg, Ret, FnStorage, $($future_storage)?>
        {
            pub fn $new<F: $($f)*>(
                f: F,
            ) -> Self {
                // SAFETY: storage is initialized with `F`
                unsafe { Self::$new_impl::<F>(FnStorage::new(f)) }
            }
        }

        #[cfg(feature = "alloc")]
        impl<'capture, Arg: ForLt, Ret: ForLt, $($future_storage: StorageMut)?> $name<'capture, Arg, Ret, crate::storage::Box, $($future_storage)?> {
            #[cfg_attr(coverage_nightly, coverage(off))]
            pub fn $new_box<F: $($f)*>(
                f: alloc::boxed::Box<F>,
            ) -> Self {
                // SAFETY: storage is initialized with `F`
                unsafe { Self::$new_impl::<F>(crate::storage::Box::new_box(f)) }
            }
        }

        crate::macros::new_impls!(@ rc $name, $fn_storage $(+ $storage_send)?, {$($f)*}, $new_impl, $new_rc $(, $future_storage)?);
        crate::macros::new_impls!(@ arc $name, $fn_storage $(+ $storage_send)?, {$($f)*}, $new_impl, $new_arc $(, $future_storage)?);

        #[cfg(feature = "alloc")]
        impl<'capture, Arg: ForLt, Ret: ForLt, const SIZE: usize, const ALIGN: usize, $($future_storage: StorageMut)?>
            $name<'capture, Arg, Ret, crate::storage::RawOrBox<SIZE, ALIGN>, $($future_storage)?>
        where
            elain::Align<ALIGN>: elain::Alignment,
        {
            #[cfg_attr(coverage_nightly, coverage(off))]
            #[cfg(feature = "alloc")]
            pub fn $new_box<F: $($f)*>(
                f: alloc::boxed::Box<F>,
            ) -> Self {
                // SAFETY: storage is initialized with `F`
                unsafe { Self::$new_impl::<F>(crate::storage::RawOrBox::new_box(f)) }
            }
        }
    };
    (@ rc $name:ident, Storage, {$($f:tt)*}, $new_impl:ident, $new_rc:ident $(, $future_storage:ident)?) => {
        #[cfg(feature = "alloc")]
        impl<'capture, Arg: ForLt, Ret: ForLt, $($future_storage: StorageMut)?> $name<'capture, Arg, Ret, crate::storage::Rc, $($future_storage)?> {
            #[cfg_attr(coverage_nightly, coverage(off))]
            pub fn $new_rc<F: $($f)*>(
                f: alloc::rc::Rc<F>,
            ) -> Self {
                // SAFETY: storage is initialized with `F`
                unsafe { Self::$new_impl::<F>(crate::storage::Rc::new_rc(f)) }
            }
        }
    };
    (@ rc $($tt:tt)*) => {};
    (@ arc $name:ident, Storage $(+ $storage_send:ident)?, {$($f:tt)*}, $new_impl:ident, $new_arc:ident $(, $future_storage:ident)?) => {
        #[cfg(feature = "alloc")]
        impl<'capture, Arg: ForLt, Ret: ForLt, $($future_storage: StorageMut)?> $name<'capture, Arg, Ret, crate::storage::Arc, $($future_storage)?> {
            #[cfg_attr(coverage_nightly, coverage(off))]
            pub fn $new_arc<F: $($f)*>(
                f: alloc::sync::Arc<F>,
            ) -> Self {
                // SAFETY: storage is initialized with `F`
                unsafe { Self::$new_impl::<F>(crate::storage::Arc::new_arc(f)) }
            }
        }
    };
    (@ arc $($tt:tt)*) => {};
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
        unsafe impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: $fn_storage + StorageSend, $($future_storage: StorageMut)?> $trait
            for $name<'capture, Arg, Ret, FnStorage, $($future_storage)?>
        {
        }
        };
}
pub(crate) use unsafe_impl_send_sync;

macro_rules! impl_clone {
    (async $name:ident, $fn_storage:ident $(+ $storage_send:ident)?) => {
        crate::macros::impl_clone!(@ $name, $fn_storage $(+ $storage_send)?, FutureStorage);
    };
    (sync $name:ident, $fn_storage:ident $(+ $storage_send:ident)?) => {
        crate::macros::impl_clone!(@ $name, $fn_storage $(+ $storage_send)?);
    };
    (@ $name:ident $(.$field:tt)?, $fn_storage:ident $(+ $storage_send:ident)? $(, $future_storage:ident)?) => {
        #[cfg(feature = "alloc")]
        impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: $fn_storage $(+ $storage_send)? + Clone, $($future_storage: StorageMut)?> Clone
            for $name<'capture, Arg, Ret, FnStorage, $($future_storage)?>
        {
            crate::macros::impl_clone!(@ clone $($storage_send)?);
        }
    };
    (@ clone StorageSend) => {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    };
    (@ clone) => {
        fn clone(&self) -> Self {
            Self {
                storage: self.storage.clone(),
                _capture: PhantomData,
            }
        }
    };
}
pub(crate) use impl_clone;

macro_rules! impl_debug {
    (async $name:ident, $fn_storage:ident $(+ $storage_send:ident)?) => {
        crate::macros::impl_debug!(@ $name, $fn_storage $(+ $storage_send)?, FutureStorage);
    };
    (sync $name:ident, $fn_storage:ident $(+ $storage_send:ident)?) => {
        crate::macros::impl_debug!(@ $name, $fn_storage $(+ $storage_send)?);
    };
    (@ $name:ident, $fn_storage:ident $(+ $storage_send:ident)? $(, $future_storage:ident)?) => {
        impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: $fn_storage $(+ $storage_send)?, $($future_storage: StorageMut)?> core::fmt::Debug
            for $name<'capture, Arg, Ret, FnStorage, $($future_storage)?>
        {
            #[cfg_attr(coverage_nightly, coverage(off))]
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.debug_struct(stringify!($name)).finish()
            }
        }

    };
}
pub(crate) use impl_debug;
