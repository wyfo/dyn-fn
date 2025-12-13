use core::{
    pin::pin,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll, Waker},
};

use dyn_fn::{hkt::*, *};

fn poll_once<R>(fut: impl Future<Output = R>) -> R {
    let fut = pin!(fut);
    match pin!(fut).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(ret) => ret,
        Poll::Pending => panic!("should not be pending"),
    }
}

struct F<'a>(&'a AtomicUsize);
impl<'capture> AsyncFnSend<'capture, ForRef<str>, ForFixed<usize>> for F<'capture> {
    async fn call<'a>(
        &self,
        arg: <ForRef<str> as ForLt>::Of<'a>,
    ) -> <ForFixed<usize> as ForLt>::Of<'a> {
        self.0.store(arg.len(), Ordering::Relaxed);
        arg.len()
    }
}
impl<'capture> AsyncFnMutSend<'capture, ForRef<str>, ForFixed<usize>> for F<'capture> {
    async fn call<'a>(
        &mut self,
        arg: <ForRef<str> as ForLt>::Of<'a>,
    ) -> <ForFixed<usize> as ForLt>::Of<'a> {
        <Self as AsyncFnSend<_, _>>::call(self, arg).await
    }
}
impl<'capture> AsyncFnOnceSend<'capture, ForRef<str>, ForFixed<usize>> for F<'capture> {
    async fn call<'a>(
        self,
        arg: <ForRef<str> as ForLt>::Of<'a>,
    ) -> <ForFixed<usize> as ForLt>::Of<'a> {
        <Self as AsyncFnSend<_, _>>::call(&self, arg).await
    }
}

#[cfg(feature = "alloc")]
type CloneStorage = storage::Arc;
#[cfg(not(feature = "alloc"))]
type CloneStorage = storage::DefaultFnStorage;

macro_rules! test {
    (sync $(($clone:ident))? $name:ident, $fn:ident) => {
        #[test]
        fn $name() {
            use core::convert::identity;
            test!(@ $(($clone))? $fn, new, identity);
        }
    };
    (async $(($clone:ident))? $name:ident, $fn:ident) => {
        #[test]
        fn $name() {
            test!(@ $(($clone))? $fn, new, poll_once, async);
            test!(@ $(($clone))? $fn, new_sync, poll_once);
        }
    };
    (async-send $(($clone:ident))? $name:ident, $fn:ident) => {
        #[test]
        fn $name() {
            let mut len = AtomicUsize::new(0);
           #[allow(unused_mut)]
            let mut callback = $fn::<ForRef<str>, ForFixed<usize>>::new(F(&len));
            assert_eq!(poll_once(callback.call("test")), 4);
            assert_eq!(*len.get_mut(), 4);
            test!(@ $(($clone))? $fn, new_sync, poll_once);
        }
    };
    (@ $(($clone:ident))? $fn:ident, $new:ident, $res:ident $(, $async:tt)?) => {
        let mut len = AtomicUsize::new(0);
        #[allow(unused_mut)]
        let mut callback = $fn::<ForRef<str>, ForFixed<usize>, test!(storage $($clone)?)>::$new($($async)?|s: &str, _| {
            len.store(s.len(), Ordering::Relaxed);
            s.len()
        });
        $(#[cfg(feature = "alloc")] let _ = callback.$clone();)?
        assert_eq!($res(callback.call("test")), 4);
        assert_eq!(*len.get_mut(), 4);
        drop($fn::<ForRef<str>, ForFixed<usize>>::$new($($async)?|s: &str, _| {
            len.store(s.len(), Ordering::Relaxed);
            s.len()
        }));
    };
    (storage clone) => {CloneStorage};
    (storage) => {storage::DefaultFnStorage}
}

test!(sync(clone) dyn_fn, DynFn);
test!(sync(clone) local_dyn_fn, LocalDynFn);
test!(sync dyn_fn_mut, DynFnMut);
test!(sync local_dyn_fn_mut, LocalDynFnMut);
test!(sync dyn_fn_once, DynFnOnce);
test!(sync local_dyn_fn_once, LocalDynFnOnce);
test!(async-send(clone) dyn_async_fn, DynAsyncFn);
test!(async(clone) local_dyn_async_fn, LocalDynAsyncFn);
test!(async-send dyn_async_fn_mut, DynAsyncFnMut);
test!(async local_dyn_async_fn_mut, LocalDynAsyncFnMut);
test!(async-send dyn_async_fn_once, DynAsyncFnOnce);
test!(async local_dyn_async_fn_once, LocalDynAsyncFnOnce);

#[test]
fn dyn_async_fn_sync() {
    let mut len = AtomicUsize::new(0);
    let callback = LocalDynAsyncFn::<ForRef<str>, ForFixed<usize>>::new_sync(|s: &str, _| {
        len.store(s.len(), Ordering::Relaxed);
        s.len()
    });
    assert_eq!(poll_once(callback.call("test")), 4);
    assert_eq!(*len.get_mut(), 4);
}
