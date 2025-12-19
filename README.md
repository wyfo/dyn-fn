## *Disclaimer*

*This crate is at an early stage of development, and is not yet released. Despite having 100% code coverage with [miri], it contains a lot of unsafe code, and there may remain some uncaught unsoundness.* 

*Documentation is available at [https://wyfo.github.io/dyn-fn/](https://wyfo.github.io/dyn-fn/). You can test the crate using a git dependency:*
```toml
dyn-fn = { git = "https://github.com/wyfo/dyn-fn" }
```

# dyn-fn

Utilities to work with `dyn [Async]Fn*`.

Think about `Box<dyn Fn>`, but with parametrizable storage (`Box`, `Arc`, etc.), and
supporting asynchronous closure!

`Raw` storage notably doesn't require allocation, making it ideally suited for
memory-constrained environments.

This crate relies on [`higher_kinded_types`], reexported as `hkt`, to support generic
lifetime in function parameters and/or return type. However, because of a [current limitation]
of the compiler, every closure requires a second `PhantomData` parameter to carry the lifetime
of the argument for the return type. This ergonomic issue doesn't impact performance.

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

This crate uses unsafe code extensively. It is fully check with [miri] to unsure the soundness of the implementation.

[`higher_kinded_types`]: https://docs.rs/higher-kinded-types/0.3.0/higher_kinded_types/
[current limitation]: https://github.com/rust-lang/rust/issues/77905
[miri]: https://github.com/rust-lang/miri