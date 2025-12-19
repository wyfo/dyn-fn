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
//! This crate relies on [`higher_kinded_types`], reexported as `hkt`, to support generic
//! lifetime in function parameters and/or return type. However, because of a [current limitation]
//! of the compiler, every closure requires a second `PhantomData` parameter to carry the lifetime
//! of the argument for the return type. This ergonomic issue doesn't impact performance.
//!
//! # Examples
//!
//! ### No allocation dynamic callback
//!
//! ```
//! #![no_std]
//! # extern crate std;
//! use dyn_fn::{LocalDynFn, hkt, storage};
//!
//! # fn main() {
//! type Callback<'a> = LocalDynFn<'a, hkt::ForRef<str>, hkt::ForFixed<()>, storage::Raw<32>>;
//! let mut callbacks = heapless::Vec::<Callback, 4>::new();
//! callbacks.push(Callback::new(|s, _| {
//!     defmt::debug!("callback called with '{}'", s)
//! }));
//!
//! let input = "input";
//! for cb in &callbacks {
//!     cb.call(input);
//! }
//! // logs "callback called with 'input'"
//! # }
//! ```
//!
//! ### Asynchronous dynamic callback
//!
//! ```
//! use std::time::Duration;
//!
//! use dyn_fn::{LocalDynAsyncFn, hkt, storage};
//! use futures_util::future::join_all;
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() {
//! type Callback<'a> = LocalDynAsyncFn<'a, hkt::ForFixed<Duration>, hkt::ForFixed<()>>;
//! let mut callbacks = Vec::<Callback>::new();
//! callbacks.push(Callback::new(async |timeout, _| {
//!     tokio::time::sleep(timeout).await
//! }));
//!
//! let timeout = Duration::from_millis(1);
//! join_all(callbacks.iter().map(|cb| cb.call(timeout))).await;
//! # }
//! ```
//!
//! [`Box`]: storage::Box
//! [`Arc`]: storage::Arc
//! [`Raw`]: storage::Raw
//! [`higher_kinded_types`]: https://docs.rs/higher-kinded-types/0.3.0/higher_kinded_types/
//! [current limitation]: https://github.com/rust-lang/rust/issues/77905

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
