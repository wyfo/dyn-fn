use core::{
    marker::PhantomData,
    mem,
    mem::ManuallyDrop,
    pin::Pin,
    ptr::NonNull,
    task::{Context, Poll},
};

use higher_kinded_types::{ForFixed, ForLt};

use crate::{
    macros::{impl_debug, new_impls, unsafe_impl_send_sync},
    storage::{
        DefaultFnStorage, DefaultFutureStorage, Storage, StorageImpl, StorageMut, StorageOnceImpl,
    },
};

pub trait AsyncFnSend<'capture, Arg: ForLt, Ret: ForLt>: Send + Sync + 'capture {
    fn call<'a>(&self, arg: Arg::Of<'a>) -> impl Future<Output = Ret::Of<'a>> + Send;
}

pub trait AsyncFnMutSend<'capture, Arg: ForLt, Ret: ForLt>: Send + Sync + 'capture {
    fn call<'a>(&mut self, arg: Arg::Of<'a>) -> impl Future<Output = Ret::Of<'a>> + Send;
}

pub trait AsyncFnOnceSend<'capture, Arg: ForLt, Ret: ForLt>: Send + Sync + 'capture {
    fn call(self, arg: Arg::Of<'_>) -> impl Future<Output = Ret::Of<'_>> + Send;
}

enum ReturnOrFuture<'a, Ret> {
    Return(Ret),
    Future(Pin<&'a mut dyn Future<Output = Ret>>),
}

impl<'a, Ret> ReturnOrFuture<'a, Ret> {
    unsafe fn store_future<F: Future<Output = Ret>, S: StorageMut>(
        fut: F,
        storage: &'a mut Option<StorageImpl<S>>,
    ) -> Self {
        let mut fut_ptr = storage.insert(StorageImpl::new(fut)).ptr_mut::<F>();
        unsafe {
            ReturnOrFuture::Future(Pin::new_unchecked(mem::transmute::<
                &mut dyn Future<Output = Ret>,
                &mut dyn Future<Output = Ret>,
            >(fut_ptr.as_mut() as _)))
        }
    }

    async fn get(self) -> Ret {
        match self {
            Self::Return(ret) => ret,
            Self::Future(future) => future.await,
        }
    }
}

#[expect(type_alias_bounds)]
type CallFn<Arg: ForLt, Ret: ForLt, FutureStorage: StorageMut> =
    for<'a, 'b> unsafe fn(
        NonNull<()>,
        Arg::Of<'a>,
        &'b mut Option<StorageImpl<FutureStorage>>,
        PhantomData<&'a ()>,
    ) -> ReturnOrFuture<'b, Ret::Of<'a>>;

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
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: Storage = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
> {
    func: StorageImpl<FnStorage>,
    call: CallFn<Arg, Ret, FutureStorage>,
    _capture: PhantomData<&'capture ()>,
}

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: Storage, FutureStorage: StorageMut>
    LocalDynAsyncFn<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    const fn new_impl<
        F: for<'a> AsyncFn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        func: StorageImpl<FnStorage>,
    ) -> Self {
        Self {
            func,
            call: |func, arg, fut, _| unsafe {
                ReturnOrFuture::store_future(func.cast::<F>().as_ref()(arg, PhantomData), fut)
            },
            _capture: PhantomData,
        }
    }

    const fn new_sync_impl<
        F: for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        func: StorageImpl<FnStorage>,
    ) -> Self {
        Self {
            func,
            call: |func, arg, _, _| unsafe {
                ReturnOrFuture::Return(func.cast::<F>().as_ref()(arg, PhantomData))
            },
            _capture: PhantomData,
        }
    }

    pub async fn call<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        let mut future = None;
        let res = unsafe { (self.call)(self.func.ptr(), arg, &mut future, PhantomData) };
        res.get().await
    }
}

new_impls!(async(arc) LocalDynAsyncFn, StorageImpl, Storage, [for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture], for<'a> AsyncFn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture);

#[cfg(feature = "alloc")]
impl<'capture, Arg: ForLt, Ret: ForLt, FutureStorage: StorageMut> Clone
    for LocalDynAsyncFn<'capture, Arg, Ret, crate::storage::Arc, FutureStorage>
{
    fn clone(&self) -> Self {
        Self {
            func: self.func.clone(),
            call: self.call,
            _capture: PhantomData,
        }
    }
}

impl_debug!(async LocalDynAsyncFn, Storage);

pub struct DynAsyncFn<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: Storage = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
>(LocalDynAsyncFn<'capture, Arg, Ret, FnStorage, FutureStorage>);

// SAFETY: the object is initialized with a `Send + Sync` function
unsafe_impl_send_sync!(async DynAsyncFn, Storage);

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: Storage, FutureStorage: StorageMut>
    DynAsyncFn<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    const fn new_impl<F: AsyncFnSend<'capture, Arg, Ret>>(func: StorageImpl<FnStorage>) -> Self {
        Self(LocalDynAsyncFn {
            func,
            call: |func, arg, fut, _| unsafe {
                ReturnOrFuture::store_future(func.cast::<F>().as_ref().call(arg), fut)
            },
            _capture: PhantomData,
        })
    }

    const fn new_sync_impl<
        F: for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        func: StorageImpl<FnStorage>,
    ) -> Self {
        Self(LocalDynAsyncFn::new_sync_impl::<F>(func))
    }

    pub async fn call<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        unsafe { SendFuture::new(self.0.call(arg)).await }
    }
}

new_impls!(async(arc) DynAsyncFn, StorageImpl, Storage, [for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture], AsyncFnSend<'capture, Arg, Ret>);

#[cfg(feature = "alloc")]
impl<'capture, Arg: ForLt, Ret: ForLt, FutureStorage: StorageMut> Clone
    for DynAsyncFn<'capture, Arg, Ret, crate::storage::Arc, FutureStorage>
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl_debug!(async DynAsyncFn.0, Storage);

pub struct LocalDynAsyncFnMut<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
> {
    func: StorageImpl<FnStorage>,
    call: CallFn<Arg, Ret, FutureStorage>,
    _capture: PhantomData<&'capture ()>,
}

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut, FutureStorage: StorageMut>
    LocalDynAsyncFnMut<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    const fn new_impl<
        F: for<'a> AsyncFnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        func: StorageImpl<FnStorage>,
    ) -> Self {
        Self {
            func,
            call: |func, arg, fut, _| unsafe {
                ReturnOrFuture::store_future(func.cast::<F>().as_mut()(arg, PhantomData), fut)
            },
            _capture: PhantomData,
        }
    }

    const fn new_sync_impl<
        F: for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        func: StorageImpl<FnStorage>,
    ) -> Self {
        Self {
            func,
            call: |func, arg, _, _| unsafe {
                ReturnOrFuture::Return(func.cast::<F>().as_mut()(arg, PhantomData))
            },
            _capture: PhantomData,
        }
    }

    pub async fn call<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        let mut future = None;
        let res = unsafe { (self.call)(self.func.ptr_mut(), arg, &mut future, PhantomData) };
        res.get().await
    }
}

new_impls!(async LocalDynAsyncFnMut, StorageImpl, StorageMut, [for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture], for<'a> AsyncFnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture);

impl_debug!(async LocalDynAsyncFnMut, StorageMut);

pub struct DynAsyncFnMut<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
>(LocalDynAsyncFnMut<'capture, Arg, Ret, FnStorage, FutureStorage>);

unsafe_impl_send_sync!(async DynAsyncFnMut, StorageMut);

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut, FutureStorage: StorageMut>
    DynAsyncFnMut<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    const fn new_impl<F: AsyncFnMutSend<'capture, Arg, Ret>>(func: StorageImpl<FnStorage>) -> Self {
        Self(LocalDynAsyncFnMut {
            func,
            call: |func, arg, fut, _| unsafe {
                ReturnOrFuture::store_future(func.cast::<F>().as_mut().call(arg), fut)
            },
            _capture: PhantomData,
        })
    }

    const fn new_sync_impl<
        F: for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        func: StorageImpl<FnStorage>,
    ) -> Self {
        Self(LocalDynAsyncFnMut::new_sync_impl::<F>(func))
    }

    pub async fn call<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        SendFuture(self.0.call(arg)).await
    }
}

new_impls!(async DynAsyncFnMut, StorageImpl, StorageMut, [for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture], AsyncFnMutSend<'capture, Arg, Ret>);

impl_debug!(async DynAsyncFnMut.0, StorageMut);

pub struct LocalDynAsyncFnOnce<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
> {
    func: ManuallyDrop<StorageOnceImpl<FnStorage>>,
    call: Option<CallFn<Arg, Ret, FutureStorage>>,
    _capture: PhantomData<&'capture ()>,
}

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut, FutureStorage: StorageMut> Drop
    for LocalDynAsyncFnOnce<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    fn drop(&mut self) {
        unsafe { ManuallyDrop::take(&mut self.func) }.drop(self.call.is_none());
    }
}

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut, FutureStorage: StorageMut>
    LocalDynAsyncFnOnce<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    const fn new_impl<
        F: for<'a> AsyncFnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        func: StorageOnceImpl<FnStorage>,
    ) -> Self {
        Self {
            func: ManuallyDrop::new(func),
            call: Some(|func, arg, fut, _| unsafe {
                ReturnOrFuture::store_future(func.cast::<F>().read()(arg, PhantomData), fut)
            }),
            _capture: PhantomData,
        }
    }

    const fn new_sync_impl<
        F: for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        func: StorageOnceImpl<FnStorage>,
    ) -> Self {
        Self {
            func: ManuallyDrop::new(func),
            call: Some(|func, arg, _, _| unsafe {
                ReturnOrFuture::Return(func.cast::<F>().read()(arg, PhantomData))
            }),
            _capture: PhantomData,
        }
    }

    pub async fn call<'a>(mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        let mut future = None;
        let call = unsafe { self.call.take().unwrap_unchecked() };
        let res = unsafe { call(self.func.ptr_once(), arg, &mut future, PhantomData) };
        res.get().await
    }
}

new_impls!(async LocalDynAsyncFnOnce, StorageOnceImpl, StorageMut, [for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture], for<'a> AsyncFnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture);

impl_debug!(async LocalDynAsyncFnOnce, StorageMut);

pub struct DynAsyncFnOnce<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
>(LocalDynAsyncFnOnce<'capture, Arg, Ret, FnStorage, FutureStorage>);

unsafe_impl_send_sync!(async DynAsyncFnOnce, StorageMut);

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut, FutureStorage: StorageMut>
    DynAsyncFnOnce<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    const fn new_impl<F: AsyncFnOnceSend<'capture, Arg, Ret>>(
        func: StorageOnceImpl<FnStorage>,
    ) -> Self {
        Self(LocalDynAsyncFnOnce {
            func: ManuallyDrop::new(func),
            call: Some(|func, arg, fut, _| unsafe {
                ReturnOrFuture::store_future(func.cast::<F>().read().call(arg), fut)
            }),
            _capture: PhantomData,
        })
    }

    const fn new_sync_impl<
        F: for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        func: StorageOnceImpl<FnStorage>,
    ) -> Self {
        Self(LocalDynAsyncFnOnce::new_sync_impl::<F>(func))
    }

    pub async fn call<'a>(self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        unsafe { SendFuture::new(self.0.call(arg)).await }
    }
}

new_impls!(async DynAsyncFnOnce, StorageOnceImpl, StorageMut, [for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture], AsyncFnOnceSend<'capture, Arg, Ret>);

impl_debug!(async DynAsyncFnOnce.0, StorageMut);
