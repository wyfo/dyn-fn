#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![no_std]
#[cfg(feature = "alloc")]
extern crate alloc;

mod r#async;
mod macros;
pub mod storage;
mod sync;

pub use r#async::{
    AsyncFnMutSend, AsyncFnOnceSend, AsyncFnSend, DynAsyncFn, DynAsyncFnMut, DynAsyncFnOnce,
    LocalDynAsyncFn, LocalDynAsyncFnMut, LocalDynAsyncFnOnce,
};
pub use higher_kinded_types as hkt;
pub use sync::{DynFn, DynFnMut, DynFnOnce, LocalDynFn, LocalDynFnMut, LocalDynFnOnce};
