use core::sync::atomic::{AtomicUsize, Ordering};

use dyn_fn::{hkt::*, *};

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
            test!(@ $(($clone))? $fn, new, call, {});
        }
    };
    (async $(($clone:ident))? $name:ident, $fn:ident) => {
        #[test]
        fn $name() {
            use futures_util::FutureExt;
            test!(@ $(($clone))? $fn, new, call, {.now_or_never().unwrap()}, async);
            test!(@ $(($clone))? $fn, new, call_try_sync, {.now_or_never().unwrap()}, async);
            test!(@ $(($clone))? $fn, new_sync, call, {.now_or_never().unwrap()});
            test!(@ $(($clone))? $fn, new_sync, call_try_sync, {.now_or_never().unwrap()});
            test!(@ $(($clone))? $fn, new_sync, call_sync, {.unwrap()});
        }
    };
    (async-send $(($clone:ident))? $name:ident, $fn:ident) => {
        #[test]
        fn $name() {
            use futures_util::FutureExt;
            let mut len = AtomicUsize::new(0);
            #[allow(unused_mut)]
            let mut callback = $fn::<ForRef<str>, ForFixed<usize>>::new(F(&len));
            assert_eq!(callback.call("test").now_or_never().unwrap(), 4);
            assert_eq!(*len.get_mut(), 4);
            let mut len = AtomicUsize::new(0);
            #[allow(unused_mut)]
            let mut callback = $fn::<ForRef<str>, ForFixed<usize>>::new(F(&len));
            assert!(!callback.is_sync());
            assert_eq!(callback.call_try_sync("test").now_or_never().unwrap(), 4);
            assert_eq!(*len.get_mut(), 4);
            test!(@ call_sync, $fn::<ForRef<str>, ForFixed<usize>>::new(F(&len)), async);
            test!(@ $(($clone))? $fn, new_sync, call, {.now_or_never().unwrap()});
            test!(@ $(($clone))? $fn, new_sync, call_try_sync, {.now_or_never().unwrap()});
            test!(@ $(($clone))? $fn, new_sync, call_sync, {.unwrap()});
        }
    };
    (@ $(($clone:ident))? $fn:ident, $new:ident, $call:ident, {$($res:tt)*} $(, $async:tt)?) => {
        let mut len = AtomicUsize::new(0);
        #[allow(unused_mut)]
        let mut callback = $fn::<ForRef<str>, ForFixed<usize>, test!(@ storage $($clone)?)>::$new($($async)?|s: &str, _| {
            len.store(s.len(), Ordering::Relaxed);
            s.len()
        });
        $(#[cfg(feature = "alloc")] let _ = callback.$clone();)?
        assert_eq!(callback.$call("test") $($res)*, 4);
        assert_eq!(*len.get_mut(), 4);
        test!(@ call_sync, $fn::<ForRef<str>, ForFixed<usize>>::$new($($async)?|s: &str, _| {
            len.store(s.len(), Ordering::Relaxed);
            s.len()
        }) $(, $async)?);
    };
    (@ storage clone) => { CloneStorage };
    (@ storage) => { storage::DefaultFnStorage };
    (@ call_sync, $callback:expr, async) => {
        #[allow(unused_mut)]
        let mut callback = $callback;
        assert!(callback.call_sync("test").is_none());
    };
    (@ call_sync, $callback:expr) => {
        drop($callback);
    };
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
