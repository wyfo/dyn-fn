use dyn_fn::{hkt::*, *};

fn assert_send<T: Send>(_: &T) {}
fn assert_sync<T: Send>(_: &T) {}

fn check_sync_fn(f: LocalDynFn<ForRef<str>, ForRef<str>>) {
    assert_send(&f);
    assert_sync(&f);
}

fn check_sync_fn_mut(f: LocalDynFnMut<ForRef<str>, ForRef<str>>) {
    assert_send(&f);
    assert_sync(&f);
}

fn check_sync_fn_once(f: LocalDynFnOnce<ForRef<str>, ForRef<str>>) {
    assert_send(&f);
    assert_sync(&f);
}

fn check_async_fn(f: LocalDynAsyncFn<ForRef<str>, ForRef<str>>) {
    assert_send(&f);
    assert_sync(&f);
    assert_send(&f.call("test"));
}

fn check_async_fn_mut(mut f: LocalDynAsyncFnMut<ForRef<str>, ForRef<str>>) {
    assert_send(&f);
    assert_sync(&f);
    assert_send(&f.call("test"));
}

fn check_async_fn_once(f: LocalDynAsyncFnOnce<ForRef<str>, ForRef<str>>) {
    assert_send(&f);
    assert_sync(&f);
    assert_send(&f.call("test"));
}

fn main() {}
