#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]
#![no_std]
#![forbid(missing_docs)]

//! Utilities to work with `dyn [Async]Fn*`.
//!
//! Think about `Box<dyn Fn>`, but with parametrizable storage ([`Box`], [`Arc`], etc.), and
//! supporting asynchronous closure!
//!
//! [`Raw`] storage notably doesn't require allocation, making it ideally suited for
//! memory-constrained environments.
//!
//! [`Box`]: storage::Box
//! [`Arc`]: storage::Arc
//! [`Raw`]: storage::Raw

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
