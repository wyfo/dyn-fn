## *Disclaimer*

*This crate is at an early stage of development, and is not yet released. Despite having 100% code coverage with [miri], it contains a lot of unsafe code, and there may remain some uncaught unsoundness.* 

*Documentation is available at [https://wyfo.github.io/dyn-fn/](https://wyfo.github.io/dyn-fn/). You can test the crate using a git dependency:*
```toml
dyn-fn = { git = "https://github.com/wyfo/dyn-fn" }
```

# dyn-fn

Utilities to work with `dyn [Async]Fn*`.

Think about `Box<dyn Fn>`, but with parametrizable storage (`Box`, `Arc`, etc.), and supporting asynchronous closure!

`Raw` storage notably doesn't require allocation, making it ideally suited for memory-constrained environments. 

This crate relies on [`higher_kinded_types`], reexported as `hkt`, to support generic lifetime in function parameters and/or return type. However, because of a [current limitation] of the compiler, every closure requires a second `PhantomData` parameter to carry the lifetime of the argument for the return type. This ergonomic issue doesn't impact performance.

`Raw` storage is the sole reason for the implementation of synchronous `DynFn`, because there is actually no difference between `Box<dyn Fn>` and `DynFn` using `Box` storage. The biggest interest of this crate lies in fact in `DynAsyncFn` implementation, and the performance improvement it offers compared to [`async_trait`] crate.

## Examples

### No allocation dynamic callback

```rust
#![no_std]
use dyn_fn::{LocalDynFn, hkt, storage};

type Callback<'a> = LocalDynFn<'a, hkt::ForRef<str>, hkt::ForFixed<()>, storage::Raw<32>>;
let mut callbacks = heapless::Vec::<Callback, 4>::new();
callbacks.push(Callback::new(|s, _| defmt::debug!("callback called with '{}'", s)));

let input = "input";
for cb in &callbacks {
    cb.call(input);
}
// logs "callback called with 'input'"
```

### Asynchronous dynamic callback

```rust
use dyn_fn::{LocalDynAsyncFn, hkt, storage};
use std::time::Duration;
use futures_util::future::join_all;

type Callback<'a> = LocalDynAsyncFn<'a  , hkt::ForFixed<Duration>, hkt::ForFixed<()>, storage::Arc>;
let mut callbacks = Vec::<Callback>::new();
callbacks.push(Callback::new(async |timeout, _| tokio::time::sleep(timeout).await));

let timeout = Duration::from_millis(1);
join_all(callbacks.iter().map(|cb| cb.call(timeout))).await;
```

## Safety

This crate uses unsafe code extensively. It is entirely checked with [miri] to unsure the soundness of the implementation.

## Benchmarks

The current way to have dynamic asynchronous function is to use [`async_trait`] crate, but it means to box every returned future. But boxing has a cost that may not be negligible when it comes to small future, like sending a value into a channel.

On the other hand, `DynAsyncFn` uses `RawOrBox` storage by default, so only big futures get allocated, while small future can be written directly on the stack — when the future maximum size is known, using `Raw` storage directly is even more performant. Moreover, this crate provides an optimized execution path for synchronous function (e.g. pushing into a ringbuffer channel) wrapped in `DynAsyncFn`.

Here are the crude results of a micro-benchmark comparing `dyn_fn` with [`async_trait`], and testing the different optimizations of `dyn_fn`:

```
async_trait                      fastest       │ slowest       │ median        │ mean          │ samples │ iters
├─ dyn_async_fn                  3.915 ns      │ 22.34 ns      │ 4.323 ns      │ 4.387 ns      │ 163300  │ 167219200
├─ dyn_async_fn_box              12.72 ns      │ 1.083 µs      │ 15.35 ns      │ 16.01 ns      │ 1677755 │ 53688160
├─ dyn_async_fn_raw              2.267 ns      │ 30.87 ns      │ 2.491 ns      │ 2.529 ns      │ 126353  │ 258770944
├─ dyn_async_fn_sync             3.956 ns      │ 45.25 ns      │ 4.363 ns      │ 4.449 ns      │ 167079  │ 171088896
├─ dyn_async_fn_sync_try         1.739 ns      │ 8.127 ns      │ 2.105 ns      │ 2.076 ns      │ 142549  │ 291940352
├─ dyn_async_fn_sync_try_manual  0.457 ns      │ 2.212 ns      │ 0.467 ns      │ 0.47 ns       │ 69951   │ 573038592
├─ dyn_async_fn_try              5.584 ns      │ 53.31 ns      │ 6.112 ns      │ 6.099 ns      │ 130154  │ 133277696
├─ dyn_async_fn_try_manual       3.997 ns      │ 19.17 ns      │ 4.485 ns      │ 4.564 ns      │ 163646  │ 167573504
╰─ dyn_async_trait               14.53 ns      │ 276.4 ns      │ 15.83 ns      │ 15.8 ns       │ 225942  │ 57841152
```

## Next steps

The implementation of this crate is in fact quite generalizable to all traits. The `storage` module should be extracted into its own `dyn_storage` crate, with a proc-macro to generate a custom `DynStorage` from a trait.

[`higher_kinded_types`]: https://docs.rs/higher-kinded-types/0.3.0/higher_kinded_types/
[current limitation]: https://github.com/rust-lang/rust/issues/77905
[miri]: https://github.com/rust-lang/miri
[async_trait]: https://docs.rs/async-trait/latest/async_trait/