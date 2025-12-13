#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![no_std]
#[cfg(feature = "alloc")]
extern crate alloc;

mod r#async;
pub mod storage;
mod sync;

pub use higher_kinded_types as hkt;
pub use r#async::{
    AsyncFnMutSend, AsyncFnOnceSend, AsyncFnSend, DynAsyncFn, DynAsyncFnMut, DynAsyncFnOnce,
    LocalDynAsyncFn, LocalDynAsyncFnMut, LocalDynAsyncFnOnce,
};
pub use sync::{DynFn, DynFnMut, DynFnOnce, LocalDynFn, LocalDynFnMut, LocalDynFnOnce};

macro_rules! impl_debug {
    (async $name:ident, $fn_storage:ident) => {
        $crate::impl_debug!(@ $name, $fn_storage, FutureStorage: StorageMut);
    };
   (sync $name:ident, $fn_storage:ident) => {
        $crate::impl_debug!(@ $name, $fn_storage);
    };
    (@ $name:ident, $fn_storage:ident $(, $future_storage:ident: $future_storage_bound:ident)?) => {
        impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: $fn_storage, $($future_storage: $future_storage_bound)?> core::fmt::Debug
            for $name<'capture, Arg, Ret, FnStorage, $($future_storage)?>
        {
            #[cfg_attr(coverage_nightly, coverage(off))]
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.debug_struct(stringify!($name))
                    .field("func", &self.0.func)
                    .field("call", &self.0.call)
                    .finish()
            }
        }

    }

}
use impl_debug;
