use dyn_fn::{hkt::*, *};

fn assert_send<T: Send>(_: &T) {}
fn assert_sync<T: Send>(_: &T) {}

fn check_sync_fn(f: DynFn<ForRef<str>, ForRef<str>>) {
    assert_send(&f);
    assert_sync(&f);
}

fn check_sync_fn_mut(f: DynFnMut<ForRef<str>, ForRef<str>>) {
    assert_send(&f);
    assert_sync(&f);
}

fn check_sync_fn_once(f: DynFnOnce<ForRef<str>, ForRef<str>>) {
    assert_send(&f);
    assert_sync(&f);
}

fn check_async_fn(f: DynAsyncFn<ForRef<str>, ForRef<str>>) {
    assert_send(&f);
    assert_sync(&f);
    assert_send(&f.call("test"));
}

fn check_async_fn_mut(mut f: DynAsyncFnMut<ForRef<str>, ForRef<str>>) {
    assert_send(&f);
    assert_sync(&f);
    assert_send(&f.call("test"));
}

fn check_async_fn_once(f: DynAsyncFnOnce<ForRef<str>, ForRef<str>>) {
    assert_send(&f);
    assert_sync(&f);
    assert_send(&f.call("test"));
}

fn main() {}
