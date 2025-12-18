use std::{
    hint::black_box,
    pin::pin,
    task::{Context, Poll, Waker},
};

use async_trait::async_trait;
use divan::Bencher;
use dyn_fn::{LocalDynAsyncFn, storage};
use higher_kinded_types::{ForFixed, ForRef};

struct Foo;
#[async_trait(?Send)]
pub trait Bar {
    async fn call(&self, arg: &str) -> usize;
}

#[async_trait(?Send)]
impl Bar for Foo {
    async fn call(&self, arg: &str) -> usize {
        arg.len()
    }
}

// `futures::future::FutureExt::now_or_never` was not inlined, and it was messing
// the results up, with `dyn_async_trait` as fast as `dyn_async_fn`
pub trait FutureExt: Future + Sized {
    #[inline(always)]
    fn now_or_never(self) -> Option<Self::Output> {
        match pin!(self).poll(&mut Context::from_waker(Waker::noop())) {
            Poll::Ready(x) => Some(x),
            _ => None,
        }
    }
}

impl<F: Future> FutureExt for F {}

#[divan::bench]
fn dyn_async_trait(b: Bencher) {
    let dyn_bar = black_box(Box::new(Foo) as Box<dyn Bar>);
    b.bench_local(|| dyn_bar.call("test").now_or_never())
}

#[divan::bench]
fn dyn_async_fn(b: Bencher) {
    let dyn_async_fn = black_box(LocalDynAsyncFn::<ForRef<str>, ForFixed<usize>>::new(
        async |s: &str, _| s.len(),
    ));
    b.bench_local(|| dyn_async_fn.call("test").now_or_never())
}

#[divan::bench]
fn dyn_async_fn_sync(b: Bencher) {
    let dyn_async_fn = black_box(LocalDynAsyncFn::<ForRef<str>, ForFixed<usize>>::new_sync(
        |s: &str, _| s.len(),
    ));
    b.bench_local(|| dyn_async_fn.call("test").now_or_never())
}

#[divan::bench]
fn dyn_async_fn_sync_shortcut(b: Bencher) {
    let dyn_async_fn = black_box(LocalDynAsyncFn::<ForRef<str>, ForFixed<usize>>::new_sync(
        |s: &str, _| s.len(),
    ));
    b.bench_local(|| {
        dyn_async_fn
            .call_sync("test")
            .or_else(|| dyn_async_fn.call("test").now_or_never())
    })
}

#[divan::bench]
fn dyn_async_fn_box(b: Bencher) {
    let dyn_async_fn = black_box(LocalDynAsyncFn::<
        ForRef<str>,
        ForFixed<usize>,
        storage::Box,
        storage::Box,
    >::new(async |s: &str, _| s.len()));
    b.bench_local(|| dyn_async_fn.call("test").now_or_never())
}

fn main() {
    divan::main();
}
