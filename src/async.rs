use core::{marker::PhantomData, mem, mem::ManuallyDrop, pin::Pin, ptr::NonNull};

use higher_kinded_types::{ForFixed, ForLt};

use crate::{
    impl_debug,
    storage::{
        DefaultFnStorage, DefaultFutureStorage, SendWrapper, Storage, StorageImpl, StorageMut,
        StorageOnceImpl,
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

enum ReturnOrFuture<'a, Ret, Fut: DynFuture> {
    Return(Ret),
    Future(Pin<&'a mut Fut::Future<Ret>>),
}

impl<Ret, Fut: DynFuture> ReturnOrFuture<'_, Ret, Fut> {
    async fn get(self) -> Ret {
        match self {
            Self::Return(ret) => ret,
            Self::Future(future) => future.await,
        }
    }
}

struct LocalDynFuture;
struct DynFutureSend;

trait DynFuture: Sized {
    type Future<Output>: Future<Output = Output> + ?Sized;
    type FutureStorage<S: StorageMut>;
    unsafe fn store_future<Ret, F: Future<Output = Ret>, S: StorageMut>(
        fut: F,
        storage: &mut Option<Self::FutureStorage<S>>,
    ) -> ReturnOrFuture<'_, Ret, Self>;
}
impl DynFuture for LocalDynFuture {
    type Future<Output> = dyn Future<Output = Output>;
    type FutureStorage<S: StorageMut> = StorageImpl<S>;

    unsafe fn store_future<Ret, F: Future<Output = Ret>, S: StorageMut>(
        fut: F,
        storage: &mut Option<Self::FutureStorage<S>>,
    ) -> ReturnOrFuture<'_, Ret, Self> {
        let fut_ptr = storage.insert(StorageImpl::new(fut)).ptr_mut::<F>();
        let dyn_fut = unsafe {
            mem::transmute::<*mut dyn Future<Output = Ret>, *mut Self::Future<Ret>>(
                fut_ptr.as_ptr() as _,
            )
        };
        unsafe { ReturnOrFuture::Future(Pin::new_unchecked(&mut *dyn_fut)) }
    }
}
impl DynFuture for DynFutureSend {
    type Future<Output> = dyn Future<Output = Output> + Send;
    type FutureStorage<S: StorageMut> = SendWrapper<StorageImpl<S>>;
    unsafe fn store_future<Ret, F: Future<Output = Ret>, S: StorageMut>(
        fut: F,
        storage: &mut Option<Self::FutureStorage<S>>,
    ) -> ReturnOrFuture<'_, Ret, Self> {
        let fut_ptr = storage
            .insert(unsafe { SendWrapper::new(StorageImpl::new(fut)) })
            .ptr_mut::<F>();
        let dyn_fut = unsafe {
            mem::transmute::<*mut dyn Future<Output = Ret>, *mut Self::Future<Ret>>(
                fut_ptr.as_ptr() as _,
            )
        };
        unsafe { ReturnOrFuture::Future(Pin::new_unchecked(&mut *dyn_fut)) }
    }
}

#[expect(type_alias_bounds)]
type CallFn<Arg: ForLt, Ret: ForLt, FutureStorage: StorageMut, Fut: DynFuture> =
    for<'a, 'b> unsafe fn(
        NonNull<()>,
        Arg::Of<'a>,
        &'b mut Option<Fut::FutureStorage<FutureStorage>>,
        PhantomData<&'a ()>,
    ) -> ReturnOrFuture<'b, Ret::Of<'a>, Fut>;

struct DynAsyncFnImpl<
    'capture,
    Arg: ForLt,
    Ret: ForLt,
    FnStorage: Storage,
    FutureStorage: StorageMut,
    Fut: DynFuture,
> {
    func: StorageImpl<FnStorage>,
    call: CallFn<Arg, Ret, FutureStorage, Fut>,
    _capture: PhantomData<&'capture ()>,
}

unsafe impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: Storage, FutureStorage: StorageMut> Send
    for DynAsyncFnImpl<'capture, Arg, Ret, FnStorage, FutureStorage, DynFutureSend>
{
}
unsafe impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: Storage, FutureStorage: StorageMut> Sync
    for DynAsyncFnImpl<'capture, Arg, Ret, FnStorage, FutureStorage, DynFutureSend>
{
}

impl<
    'capture,
    Arg: ForLt,
    Ret: ForLt,
    FnStorage: Storage,
    FutureStorage: StorageMut,
    Fut: DynFuture,
> DynAsyncFnImpl<'capture, Arg, Ret, FnStorage, FutureStorage, Fut>
{
    fn new<F: for<'a> AsyncFn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture>(
        f: F,
    ) -> Self {
        Self {
            func: StorageImpl::new(f),
            call: |func, arg, fut, _| unsafe {
                Fut::store_future(func.cast::<F>().as_ref()(arg, PhantomData), fut)
            },
            _capture: PhantomData,
        }
    }

    fn new_mut<
        F: for<'a> AsyncFnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        f: F,
    ) -> Self {
        Self {
            func: StorageImpl::new(f),
            call: |func, arg, fut, _| unsafe {
                Fut::store_future(func.cast::<F>().as_mut()(arg, PhantomData), fut)
            },
            _capture: PhantomData,
        }
    }

    fn new_sync<F: for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture>(
        f: F,
    ) -> Self {
        Self {
            func: StorageImpl::new(f),
            call: |func, arg, _, _| unsafe {
                ReturnOrFuture::Return(func.cast::<F>().as_ref()(arg, PhantomData))
            },
            _capture: PhantomData,
        }
    }

    fn new_mut_sync<
        F: for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        f: F,
    ) -> Self {
        Self {
            func: StorageImpl::new(f),
            call: |func, arg, _, _| unsafe {
                ReturnOrFuture::Return(func.cast::<F>().as_mut()(arg, PhantomData))
            },
            _capture: PhantomData,
        }
    }

    pub async unsafe fn call<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        let mut future = None;
        let res = unsafe { (self.call)(self.func.ptr(), arg, &mut future, PhantomData) };
        res.get().await
    }

    pub async unsafe fn call_mut<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        let mut future = None;
        let res = unsafe { (self.call)(self.func.ptr_mut(), arg, &mut future, PhantomData) };
        res.get().await
    }
}

pub struct DynAsyncFn<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: Storage = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
>(DynAsyncFnImpl<'capture, Arg, Ret, FnStorage, FutureStorage, DynFutureSend>);

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: Storage, FutureStorage: StorageMut>
    DynAsyncFn<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    pub fn new<F: AsyncFnSend<'capture, Arg, Ret>>(f: F) -> Self {
        Self(DynAsyncFnImpl::new(async move |arg, _| f.call(arg).await))
    }

    pub fn new_sync<
        F: for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        f: F,
    ) -> Self {
        Self(DynAsyncFnImpl::new_sync(f))
    }

    pub async fn call<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        unsafe { self.0.call(arg).await }
    }
}

#[cfg(feature = "alloc")]
impl<'capture, Arg: ForLt, Ret: ForLt, FutureStorage: StorageMut> Clone
    for DynAsyncFn<'capture, Arg, Ret, crate::storage::Arc, FutureStorage>
{
    fn clone(&self) -> Self {
        Self(DynAsyncFnImpl {
            func: self.0.func.clone(),
            call: self.0.call,
            _capture: PhantomData,
        })
    }
}

impl_debug!(async DynAsyncFn, Storage);

pub struct LocalDynAsyncFn<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: Storage = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
>(DynAsyncFnImpl<'capture, Arg, Ret, FnStorage, FutureStorage, LocalDynFuture>);

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: Storage, FutureStorage: StorageMut>
    LocalDynAsyncFn<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    pub fn new<F: for<'a> AsyncFn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture>(
        f: F,
    ) -> Self {
        Self(DynAsyncFnImpl::new(f))
    }

    pub fn new_sync<F: for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture>(
        f: F,
    ) -> Self {
        Self(DynAsyncFnImpl::new_sync(f))
    }

    pub async fn call<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        unsafe { self.0.call(arg).await }
    }
}

#[cfg(feature = "alloc")]
impl<'capture, Arg: ForLt, Ret: ForLt, FutureStorage: StorageMut> Clone
    for LocalDynAsyncFn<'capture, Arg, Ret, crate::storage::Arc, FutureStorage>
{
    fn clone(&self) -> Self {
        Self(DynAsyncFnImpl {
            func: self.0.func.clone(),
            call: self.0.call,
            _capture: PhantomData,
        })
    }
}

impl_debug!(async LocalDynAsyncFn, Storage);

pub struct DynAsyncFnMut<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
>(DynAsyncFnImpl<'capture, Arg, Ret, FnStorage, FutureStorage, DynFutureSend>);

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut, FutureStorage: StorageMut>
    DynAsyncFnMut<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    pub fn new<F: AsyncFnMutSend<'capture, Arg, Ret>>(mut f: F) -> Self {
        Self(DynAsyncFnImpl::new_mut(async move |arg, _| {
            f.call(arg).await
        }))
    }

    pub fn new_sync<
        F: for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        f: F,
    ) -> Self {
        Self(DynAsyncFnImpl::new_mut_sync(f))
    }

    pub async fn call<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        unsafe { self.0.call_mut(arg).await }
    }
}

impl_debug!(async DynAsyncFnMut, StorageMut);

pub struct LocalDynAsyncFnMut<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
>(DynAsyncFnImpl<'capture, Arg, Ret, FnStorage, FutureStorage, LocalDynFuture>);

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut, FutureStorage: StorageMut>
    LocalDynAsyncFnMut<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    pub fn new<
        F: for<'a> AsyncFnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        f: F,
    ) -> Self {
        Self(DynAsyncFnImpl::new_mut(f))
    }

    pub fn new_sync<
        F: for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        f: F,
    ) -> Self {
        Self(DynAsyncFnImpl::new_mut_sync(f))
    }

    pub async fn call<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        unsafe { self.0.call_mut(arg).await }
    }
}

impl_debug!(async LocalDynAsyncFnMut, StorageMut);

struct DynAsyncFnOnceImpl<
    'capture,
    Arg: ForLt,
    Ret: ForLt,
    FnStorage: StorageMut,
    FutureStorage: StorageMut,
    Fut: DynFuture,
> {
    func: ManuallyDrop<StorageOnceImpl<FnStorage>>,
    call: Option<CallFn<Arg, Ret, FutureStorage, Fut>>,
    _capture: PhantomData<&'capture ()>,
}

unsafe impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut, FutureStorage: StorageMut> Send
    for DynAsyncFnOnceImpl<'capture, Arg, Ret, FnStorage, FutureStorage, DynFutureSend>
{
}
unsafe impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut, FutureStorage: StorageMut> Sync
    for DynAsyncFnOnceImpl<'capture, Arg, Ret, FnStorage, FutureStorage, DynFutureSend>
{
}

impl<
    'capture,
    Arg: ForLt,
    Ret: ForLt,
    FnStorage: StorageMut,
    FutureStorage: StorageMut,
    Fut: DynFuture,
> Drop for DynAsyncFnOnceImpl<'capture, Arg, Ret, FnStorage, FutureStorage, Fut>
{
    fn drop(&mut self) {
        unsafe { ManuallyDrop::take(&mut self.func) }.drop(self.call.is_none());
    }
}

impl<
    'capture,
    Arg: ForLt,
    Ret: ForLt,
    FnStorage: StorageMut,
    FutureStorage: StorageMut,
    Fut: DynFuture,
> DynAsyncFnOnceImpl<'capture, Arg, Ret, FnStorage, FutureStorage, Fut>
{
    fn new<F: for<'a> AsyncFnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture>(
        f: F,
    ) -> Self {
        Self {
            func: ManuallyDrop::new(StorageOnceImpl::new(f)),
            call: Some(|func, arg, fut, _| unsafe {
                Fut::store_future(func.cast::<F>().read()(arg, PhantomData), fut)
            }),
            _capture: PhantomData,
        }
    }

    fn new_sync<F: for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture>(
        f: F,
    ) -> Self {
        Self {
            func: ManuallyDrop::new(StorageOnceImpl::new(f)),
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

pub struct DynAsyncFnOnce<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
>(DynAsyncFnOnceImpl<'capture, Arg, Ret, FnStorage, FutureStorage, DynFutureSend>);

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut, FutureStorage: StorageMut>
    DynAsyncFnOnce<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    pub fn new<F: AsyncFnOnceSend<'capture, Arg, Ret>>(f: F) -> Self {
        Self(DynAsyncFnOnceImpl::new(async move |arg, _| {
            f.call(arg).await
        }))
    }

    pub fn new_sync<
        F: for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        f: F,
    ) -> Self {
        Self(DynAsyncFnOnceImpl::new_sync(f))
    }

    pub async fn call<'a>(self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        self.0.call(arg).await
    }
}

impl_debug!(async DynAsyncFnOnce, StorageMut);

pub struct LocalDynAsyncFnOnce<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
>(DynAsyncFnOnceImpl<'capture, Arg, Ret, FnStorage, FutureStorage, LocalDynFuture>);

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut, FutureStorage: StorageMut>
    LocalDynAsyncFnOnce<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    pub fn new<
        F: for<'a> AsyncFnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        f: F,
    ) -> Self {
        Self(DynAsyncFnOnceImpl::new(f))
    }

    pub fn new_sync<
        F: for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        f: F,
    ) -> Self {
        Self(DynAsyncFnOnceImpl::new_sync(f))
    }

    pub async fn call<'a>(self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        self.0.call(arg).await
    }
}

impl_debug!(async LocalDynAsyncFnOnce, StorageMut);
