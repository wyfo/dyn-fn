use std::hint::black_box;

use divan::Bencher;
use dyn_fn::LocalDynFn;
use higher_kinded_types::{ForFixed, ForRef};

#[divan::bench]
fn box_dyn_fn(b: Bencher) {
    let f = black_box(Box::new(|s: &str| s.len()) as Box<dyn Fn(&str) -> usize>);
    b.bench_local(|| f("test"))
}

#[divan::bench]
fn dyn_fn(b: Bencher) {
    let f = black_box(LocalDynFn::<ForRef<str>, ForFixed<usize>>::new(
        |s: &str, _| s.len(),
    ));
    b.bench_local(|| f.call("test"))
}

fn main() {
    divan::main();
}
