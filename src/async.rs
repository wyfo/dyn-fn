use core::{
    future::poll_fn,
    marker::PhantomData,
    mem,
    mem::{ManuallyDrop, MaybeUninit},
    pin::Pin,
    ptr::NonNull,
    task::{Context, Poll},
};

use higher_kinded_types::{ForFixed, ForLt};

use crate::{
    macros::{impl_debug, new_impls, unsafe_impl_send_sync},
    storage::{
        DefaultFnStorage, DefaultFutureStorage, DropVTable, DynStorage, Storage, StorageMoved,
        StorageMut, VTable,
    },
};

pub trait AsyncFnSend<'capture, Arg: ForLt + 'static, Ret: ForLt>: Send + Sync + 'capture {
    fn call<'a>(&self, arg: Arg::Of<'a>) -> impl Future<Output = Ret::Of<'a>> + Send;
}

pub trait AsyncFnMutSend<'capture, Arg: ForLt + 'static, Ret: ForLt>:
    Send + Sync + 'capture
{
    fn call<'a>(&mut self, arg: Arg::Of<'a>) -> impl Future<Output = Ret::Of<'a>> + Send;
}

pub trait AsyncFnOnceSend<'capture, Arg: ForLt + 'static, Ret: ForLt>:
    Send + Sync + 'capture
{
    fn call(self, arg: Arg::Of<'_>) -> impl Future<Output = Ret::Of<'_>> + Send;
}

#[expect(type_alias_bounds)]
type PollFn<Ret: ForLt> =
    for<'a> unsafe fn(NonNull<()>, &mut Context<'_>, PhantomData<&'a ()>) -> Poll<Ret::Of<'a>>;

struct FutureVTable<Ret: ForLt> {
    poll: PollFn<Ret>,
    drop_vtable: DropVTable,
}

#[cfg_attr(coverage_nightly, coverage(off))]
fn store_future<
    'a,
    Ret: ForLt + 'static,
    FutureStorage: StorageMut,
    Fut: Future<Output = Ret::Of<'a>>,
>(
    storage: &mut MaybeUninit<FutureStorage>,
    future: Fut,
) -> &'static FutureVTable<Ret> {
    storage.write(FutureStorage::new(future));
    &FutureVTable {
        poll: |fut, cx, _| unsafe {
            mem::transmute::<Poll<Ret::Of<'a>>, Poll<Ret::Of<'_>>>(
                Pin::new_unchecked(fut.cast::<Fut>().as_mut()).poll(cx),
            )
        },
        drop_vtable: const { DropVTable::new::<FutureStorage, Fut>() },
    }
}

async unsafe fn async_call<'a, FutureStorage: StorageMut, Ret: ForLt + 'static>(
    call: impl FnOnce(&mut MaybeUninit<FutureStorage>) -> &'static FutureVTable<Ret>,
) -> Ret::Of<'a> {
    let mut future = MaybeUninit::<FutureStorage>::uninit();
    let vtable = call(&mut future);
    let future = future.as_mut_ptr();
    struct DropGuard<F: FnMut()>(F);
    impl<F: FnMut()> Drop for DropGuard<F> {
        fn drop(&mut self) {
            self.0();
        }
    }
    let _guard = DropGuard(|| unsafe { vtable.drop_vtable.drop_storage(&mut *future) });
    poll_fn(|cx| unsafe { (vtable.poll)((*future).ptr_mut(), cx, PhantomData) }).await
}

#[expect(type_alias_bounds)]
type Call<Arg: ForLt + 'static, Ret: ForLt + 'static, FutureStorage> =
    for<'a> unsafe fn(
        NonNull<()>,
        Arg::Of<'a>,
        &mut MaybeUninit<FutureStorage>,
        PhantomData<&'a ()>,
    ) -> &'static FutureVTable<Ret>;

#[expect(type_alias_bounds)]
type CallSync<Arg: ForLt + 'static, Ret: ForLt> =
    for<'a, 'b> unsafe fn(NonNull<()>, Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a>;

struct AsyncVTable<Arg: ForLt + 'static, Ret: ForLt + 'static, FutureStorage> {
    call: Call<Arg, Ret, FutureStorage>,
    call_sync: Option<CallSync<Arg, Ret>>,
    drop_vtable: DropVTable,
}

impl<Arg: ForLt + 'static, Ret: ForLt + 'static, FutureStorage: StorageMut> VTable
    for AsyncVTable<Arg, Ret, FutureStorage>
{
    fn drop_vtable(&self) -> &DropVTable {
        &self.drop_vtable
    }
}
struct SendFuture<F>(F);
impl<F> SendFuture<F> {
    unsafe fn new(future: F) -> Self {
        Self(future)
    }
}

unsafe impl<F> Send for SendFuture<F> {}
impl<F: Future> Future for SendFuture<F> {
    type Output = F::Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        unsafe { self.map_unchecked_mut(|this| &mut this.0) }.poll(cx)
    }
}

pub struct LocalDynAsyncFn<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: Storage = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
> {
    storage: DynStorage<FnStorage, AsyncVTable<Arg, Ret, FutureStorage>>,
    _capture: PhantomData<&'capture ()>,
}

impl<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static,
    FnStorage: Storage,
    FutureStorage: StorageMut,
> LocalDynAsyncFn<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    const fn new_impl<
        F: for<'a> AsyncFn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        Self {
            storage: DynStorage {
                storage,
                vtable: &AsyncVTable {
                    call: |func, arg, fut, _| {
                        store_future(fut, unsafe { func.cast::<F>().as_ref()(arg, PhantomData) })
                    },
                    call_sync: None,
                    drop_vtable: const { DropVTable::new::<FnStorage, F>() },
                },
            },
            _capture: PhantomData,
        }
    }

    const fn new_sync_impl<
        F: for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        Self {
            storage: DynStorage {
                storage,
                vtable: &AsyncVTable {
                    call: |func, arg, fut, _| {
                        store_future(fut, async move {
                            unsafe { func.cast::<F>().as_ref()(arg, PhantomData) }
                        })
                    },
                    call_sync: Some(|func, arg, _| unsafe {
                        func.cast::<F>().as_ref()(arg, PhantomData)
                    }),
                    drop_vtable: const { DropVTable::new::<FnStorage, F>() },
                },
            },
            _capture: PhantomData,
        }
    }

    pub async fn call<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        let call = |fut: &mut _| unsafe {
            (self.storage.vtable.call)(self.storage.ptr(), arg, fut, PhantomData)
        };
        unsafe { async_call(call) }.await
    }

    // TODO I've no idea why this code is not fully covered when alloc feature is enabled
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn call_sync<'a>(&self, arg: Arg::Of<'a>) -> Option<Ret::Of<'a>> {
        unsafe { self.storage.vtable.call_sync?(self.storage.ptr(), arg, PhantomData) }.into()
    }
}

new_impls!(async(arc) LocalDynAsyncFn, Storage, [for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture], for<'a> AsyncFn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture);

#[cfg(feature = "alloc")]
impl<'capture, Arg: ForLt + 'static, Ret: ForLt + 'static, FutureStorage: StorageMut> Clone
    for LocalDynAsyncFn<'capture, Arg, Ret, crate::storage::Arc, FutureStorage>
{
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            _capture: PhantomData,
        }
    }
}

impl_debug!(async LocalDynAsyncFn, Storage);

pub struct DynAsyncFn<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: Storage = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
>(LocalDynAsyncFn<'capture, Arg, Ret, FnStorage, FutureStorage>);

// SAFETY: the object is initialized with a `Send + Sync` function
unsafe_impl_send_sync!(async DynAsyncFn, Storage);

impl<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static,
    FnStorage: Storage,
    FutureStorage: StorageMut,
> DynAsyncFn<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    const fn new_impl<F: AsyncFnSend<'capture, Arg, Ret>>(storage: FnStorage) -> Self {
        Self(LocalDynAsyncFn {
            storage: DynStorage {
                storage,
                vtable: &AsyncVTable {
                    call: |func, arg, fut, _| {
                        store_future(fut, unsafe { func.cast::<F>().as_ref().call(arg) })
                    },
                    call_sync: None,
                    drop_vtable: const { DropVTable::new::<FnStorage, F>() },
                },
            },
            _capture: PhantomData,
        })
    }

    const fn new_sync_impl<
        F: for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        Self(LocalDynAsyncFn::new_sync_impl::<F>(storage))
    }

    pub async fn call<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        unsafe { SendFuture::new(self.0.call(arg)).await }
    }

    pub fn call_sync<'a>(&self, arg: Arg::Of<'a>) -> Option<Ret::Of<'a>> {
        self.0.call_sync(arg)
    }
}

new_impls!(async(arc) DynAsyncFn, Storage, [for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture], AsyncFnSend<'capture, Arg, Ret>);

#[cfg(feature = "alloc")]
impl<'capture, Arg: ForLt + 'static, Ret: ForLt + 'static, FutureStorage: StorageMut> Clone
    for DynAsyncFn<'capture, Arg, Ret, crate::storage::Arc, FutureStorage>
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl_debug!(async DynAsyncFn, Storage);

pub struct LocalDynAsyncFnMut<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
> {
    storage: DynStorage<FnStorage, AsyncVTable<Arg, Ret, FutureStorage>>,
    _capture: PhantomData<&'capture ()>,
}

impl<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static,
    FnStorage: StorageMut,
    FutureStorage: StorageMut,
> LocalDynAsyncFnMut<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    const fn new_impl<
        F: for<'a> AsyncFnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        Self {
            storage: DynStorage {
                storage,
                vtable: &AsyncVTable {
                    call: |func, arg, fut, _| {
                        store_future(fut, unsafe { func.cast::<F>().as_mut()(arg, PhantomData) })
                    },
                    call_sync: None,
                    drop_vtable: const { DropVTable::new::<FnStorage, F>() },
                },
            },
            _capture: PhantomData,
        }
    }

    const fn new_sync_impl<
        F: for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        Self {
            storage: DynStorage {
                storage,
                vtable: &AsyncVTable {
                    call: |func, arg, fut, _| {
                        store_future(fut, async move {
                            unsafe { func.cast::<F>().as_mut()(arg, PhantomData) }
                        })
                    },
                    call_sync: Some(|func, arg, _| unsafe {
                        func.cast::<F>().as_mut()(arg, PhantomData)
                    }),
                    drop_vtable: const { DropVTable::new::<FnStorage, F>() },
                },
            },
            _capture: PhantomData,
        }
    }

    pub async fn call<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        let call = |fut: &mut _| unsafe {
            (self.storage.vtable.call)(self.storage.ptr_mut(), arg, fut, PhantomData)
        };
        unsafe { async_call(call) }.await
    }

    pub fn call_sync<'a>(&mut self, arg: Arg::Of<'a>) -> Option<Ret::Of<'a>> {
        unsafe { self.storage.vtable.call_sync?(self.storage.ptr_mut(), arg, PhantomData) }.into()
    }
}

new_impls!(async LocalDynAsyncFnMut, StorageMut, [for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture], for<'a> AsyncFnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture);

impl_debug!(async LocalDynAsyncFnMut, StorageMut);

pub struct DynAsyncFnMut<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
>(LocalDynAsyncFnMut<'capture, Arg, Ret, FnStorage, FutureStorage>);

unsafe_impl_send_sync!(async DynAsyncFnMut, StorageMut);

impl<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static,
    FnStorage: StorageMut,
    FutureStorage: StorageMut,
> DynAsyncFnMut<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    const fn new_impl<F: AsyncFnMutSend<'capture, Arg, Ret>>(storage: FnStorage) -> Self {
        Self(LocalDynAsyncFnMut {
            storage: DynStorage {
                storage,
                vtable: &AsyncVTable {
                    call: |func, arg, fut, _| {
                        store_future(fut, unsafe { func.cast::<F>().as_mut().call(arg) })
                    },
                    call_sync: None,
                    drop_vtable: const { DropVTable::new::<FnStorage, F>() },
                },
            },
            _capture: PhantomData,
        })
    }

    const fn new_sync_impl<
        F: for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        Self(LocalDynAsyncFnMut::new_sync_impl::<F>(storage))
    }

    pub async fn call<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        SendFuture(self.0.call(arg)).await
    }

    pub fn call_sync<'a>(&mut self, arg: Arg::Of<'a>) -> Option<Ret::Of<'a>> {
        self.0.call_sync(arg)
    }
}

new_impls!(async DynAsyncFnMut, StorageMut, [for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture], AsyncFnMutSend<'capture, Arg, Ret>);

impl_debug!(async DynAsyncFnMut, StorageMut);

pub struct LocalDynAsyncFnOnce<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
> {
    storage: DynStorage<FnStorage, AsyncVTable<Arg, Ret, FutureStorage>>,
    _capture: PhantomData<&'capture ()>,
}

impl<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static,
    FnStorage: StorageMut,
    FutureStorage: StorageMut,
> LocalDynAsyncFnOnce<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    const fn new_impl<
        F: for<'a> AsyncFnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        Self {
            storage: DynStorage {
                storage,
                vtable: &AsyncVTable {
                    call: |func, arg, fut, _| {
                        store_future(fut, unsafe {
                            StorageMoved::<FnStorage, F>::new(func).read()(arg, PhantomData)
                        })
                    },
                    call_sync: None,
                    drop_vtable: const { DropVTable::new::<FnStorage, F>() },
                },
            },
            _capture: PhantomData,
        }
    }

    const fn new_sync_impl<
        F: for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        Self {
            storage: DynStorage {
                storage,
                vtable: &AsyncVTable {
                    call: |func, arg, fut, _| {
                        store_future(fut, async move {
                            unsafe {
                                StorageMoved::<FnStorage, F>::new(func).read()(arg, PhantomData)
                            }
                        })
                    },
                    call_sync: Some(|func, arg, _| unsafe {
                        StorageMoved::<FnStorage, F>::new(func).read()(arg, PhantomData)
                    }),
                    drop_vtable: const { DropVTable::new::<FnStorage, F>() },
                },
            },
            _capture: PhantomData,
        }
    }

    pub async fn call<'a>(self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        let mut storage = ManuallyDrop::new(self.storage);
        let call = |fut: &mut _| unsafe {
            (storage.vtable.call)(storage.ptr_mut(), arg, fut, PhantomData)
        };
        unsafe { async_call(call) }.await
    }

    pub fn call_sync(self, arg: Arg::Of<'_>) -> Option<Ret::Of<'_>> {
        let call_sync = self.storage.vtable.call_sync?;
        let mut storage = ManuallyDrop::new(self.storage);
        unsafe { call_sync(storage.ptr_mut(), arg, PhantomData) }.into()
    }
}

new_impls!(async LocalDynAsyncFnOnce, StorageMut, [for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture], for<'a> AsyncFnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture);

impl_debug!(async LocalDynAsyncFnOnce, StorageMut);

pub struct DynAsyncFnOnce<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
>(LocalDynAsyncFnOnce<'capture, Arg, Ret, FnStorage, FutureStorage>);

unsafe_impl_send_sync!(async DynAsyncFnOnce, StorageMut);

impl<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static,
    FnStorage: StorageMut,
    FutureStorage: StorageMut,
> DynAsyncFnOnce<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    const fn new_impl<F: AsyncFnOnceSend<'capture, Arg, Ret>>(storage: FnStorage) -> Self {
        Self(LocalDynAsyncFnOnce {
            storage: DynStorage {
                storage,
                vtable: &AsyncVTable {
                    call: |func, arg, fut, _| {
                        store_future(fut, unsafe {
                            StorageMoved::<FnStorage, F>::new(func).read().call(arg)
                        })
                    },
                    call_sync: None,
                    drop_vtable: const { DropVTable::new::<FnStorage, F>() },
                },
            },
            _capture: PhantomData,
        })
    }

    const fn new_sync_impl<
        F: for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        Self(LocalDynAsyncFnOnce::new_sync_impl::<F>(storage))
    }

    pub async fn call<'a>(self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        unsafe { SendFuture::new(self.0.call(arg)).await }
    }

    pub fn call_sync(self, arg: Arg::Of<'_>) -> Option<Ret::Of<'_>> {
        self.0.call_sync(arg)
    }
}

new_impls!(async DynAsyncFnOnce, StorageMut, [for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture], AsyncFnOnceSend<'capture, Arg, Ret>);

impl_debug!(async DynAsyncFnOnce, StorageMut);
