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
    macros::{impl_clone, impl_debug, new_impls, unsafe_impl_send_sync},
    storage::{
        DefaultFnStorage, DefaultFutureStorage, DropVTable, DynStorage, Storage, StorageMoved,
        StorageMut, StorageSend, VTable,
    },
};

/// A [`Send`] + [`Sync`] [`AsyncFn`] whose returned future is [`Send`]
pub trait AsyncFnSend<'capture, Arg: ForLt + 'static, Ret: ForLt>: Send + Sync + 'capture {
    /// Calls the function, returns a borrowed future.
    fn call<'a>(&self, arg: Arg::Of<'a>) -> impl Future<Output = Ret::Of<'a>> + Send;
}

/// A [`Send`] + [`Sync`] [`AsyncFnMut`] whose returned future is [`Send`]
pub trait AsyncFnMutSend<'capture, Arg: ForLt + 'static, Ret: ForLt>:
    Send + Sync + 'capture
{
    /// Calls the function, returns a borrowed future.
    fn call<'a>(&mut self, arg: Arg::Of<'a>) -> impl Future<Output = Ret::Of<'a>> + Send;
}

/// A [`Send`] + [`Sync`] [`AsyncFnOnce`] whose returned future is [`Send`]
pub trait AsyncFnOnceSend<'capture, Arg: ForLt + 'static, Ret: ForLt>:
    Send + Sync + 'capture
{
    /// Calls the function, returns a borrowed future.
    fn call(self, arg: Arg::Of<'_>) -> impl Future<Output = Ret::Of<'_>> + Send;
}

#[expect(type_alias_bounds)]
type PollFn<Ret: ForLt> =
    for<'a> fn(NonNull<()>, &mut Context<'_>, PhantomData<&'a ()>) -> Poll<Ret::Of<'a>>;

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
        // SAFETY: `poll` is called in poll_future, and
        // - `fut` is the future `Fut` written in the storage
        // - the lifetime passed is the real one, so it can be transmuted
        // - the future is never moved during the polling
        poll: |fut, cx, _| unsafe {
            mem::transmute::<Poll<Ret::Of<'a>>, Poll<Ret::Of<'_>>>(
                Pin::new_unchecked(fut.cast::<Fut>().as_mut()).poll(cx),
            )
        },
        drop_vtable: const { DropVTable::new::<FutureStorage, Fut>() },
    }
}

/// # Safety
///
/// `future` must be initialized, and `vtable` must match the data stored in `future`.
async unsafe fn poll_future<'a, FutureStorage: StorageMut, Ret: ForLt + 'static>(
    vtable: &'static FutureVTable<Ret>,
    future: &mut MaybeUninit<FutureStorage>,
) -> Ret::Of<'a> {
    let future = future.as_mut_ptr();
    struct DropGuard<F: FnMut()>(F);
    impl<F: FnMut()> Drop for DropGuard<F> {
        fn drop(&mut self) {
            self.0();
        }
    }
    // SAFETY: `future` is initialized and `vtable` matches the future stored;
    // the storage is no longer accessed after the call (because it's dropped)
    let _guard = DropGuard(|| unsafe { vtable.drop_vtable.drop_storage(&mut *future) });
    // SAFETY: `future` is initialized
    poll_fn(|cx| unsafe { (vtable.poll)((*future).ptr_mut(), cx, PhantomData) }).await
}

#[expect(type_alias_bounds)]
type Call<Arg: ForLt, Ret: ForLt + 'static, FutureStorage, T> =
    for<'a> fn(
        NonNull<T>,
        Arg::Of<'a>,
        &mut MaybeUninit<FutureStorage>,
        PhantomData<&'a ()>,
    ) -> &'static FutureVTable<Ret>;

#[expect(type_alias_bounds)]
type CallSync<Arg: ForLt, Ret: ForLt, T> =
    for<'a, 'b> fn(NonNull<T>, Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a>;

struct AsyncVTable<Arg: ForLt, Ret: ForLt + 'static, FutureStorage, T: 'static = ()> {
    call: Call<Arg, Ret, FutureStorage, T>,
    call_sync: Option<CallSync<Arg, Ret, T>>,
    drop_vtable: DropVTable,
}

impl<Arg: ForLt + 'static, Ret: ForLt + 'static, FutureStorage: StorageMut, T: 'static> VTable
    for AsyncVTable<Arg, Ret, FutureStorage, T>
{
    fn drop_vtable(&self) -> &DropVTable {
        &self.drop_vtable
    }
}
struct SendFuture<F>(F);
impl<F> SendFuture<F> {
    /// # Safety
    ///
    /// `future` must implement `Send`.
    unsafe fn new(future: F) -> Self {
        Self(future)
    }
}

// SAFETY: `SendFuture` is a wrapper around `F`, which implements `Send` as per `SendFuture::new`
unsafe impl<F> Send for SendFuture<F> {}
impl<F: Future> Future for SendFuture<F> {
    type Output = F::Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // SAFETY: pin projection
        unsafe { self.map_unchecked_mut(|this| &mut this.0) }.poll(cx)
    }
}

/// [`DynAsyncFn`], but without the [`Send`] + [`Sync`] requirement.
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
    /// # Safety
    ///
    /// `storage` must have been initialized with `F`.
    const unsafe fn new_impl<
        F: for<'a> AsyncFn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        let vtable = &AsyncVTable {
            call: |func, arg, fut, _| {
                // SAFETY: func comes from `self.storage.ptr()`, so it's a valid `&F`
                store_future(fut, unsafe { func.cast::<F>().as_ref()(arg, PhantomData) })
            },
            call_sync: None,
            drop_vtable: const { DropVTable::new::<FnStorage, F>() },
        };
        Self {
            // SAFETY: `drop_vtable` matches the storage
            storage: unsafe { DynStorage::new(storage, vtable) },
            _capture: PhantomData,
        }
    }

    /// # Safety
    ///
    /// `storage` must have been initialized with `F`.
    const unsafe fn new_sync_impl<
        F: for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        let vtable = &AsyncVTable {
            call: |func, arg, fut, _| {
                store_future(fut, async move {
                    // SAFETY: func comes from `self.storage.ptr()`, so it's a valid `&F`
                    unsafe { func.cast::<F>().as_ref()(arg, PhantomData) }
                })
            },
            // SAFETY: func comes from `self.storage.ptr()`, so it's a valid `&F`
            call_sync: Some(|func, arg, _| unsafe { func.cast::<F>().as_ref()(arg, PhantomData) }),
            drop_vtable: const { DropVTable::new::<FnStorage, F>() },
        };
        Self {
            // SAFETY: `drop_vtable` matches the storage
            storage: unsafe { DynStorage::new(storage, vtable) },
            _capture: PhantomData,
        }
    }

    /// Returns whether the underlying function is synchronous.
    pub fn is_sync(&self) -> bool {
        self.storage.vtable().call_sync.is_some()
    }

    /// Calls the underlying function.
    pub async fn call<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        let mut future = MaybeUninit::uninit();
        let vtable =
            (self.storage.vtable().call)(self.storage.ptr(), arg, &mut future, PhantomData);
        // SAFETY: `future` has been initialized in `call`, and the vtable
        // returned by `store_future` matches the future stored
        unsafe { poll_future(vtable, &mut future) }.await
    }

    /// Calls the underlying function if is synchronous.
    // TODO I've no idea why this code is not fully covered when alloc feature is enabled
    // Anyway, it surely comes from https://github.com/taiki-e/cargo-llvm-cov/issues/394
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub fn call_sync<'a>(&self, arg: Arg::Of<'a>) -> Option<Ret::Of<'a>> {
        self.storage.vtable().call_sync?(self.storage.ptr(), arg, PhantomData).into()
    }

    /// Tries calling the underlying function as synchronous, falling back to asynchronous call.
    ///
    /// This is equivalent to
    /// ```ignore
    /// if self.is_sync() {
    ///     self.call_sync(arg).unwrap()
    /// } else {
    ///     self.call(arg).await
    /// }
    /// ```
    pub async fn call_try_sync<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        if self.is_sync() {
            self.call_sync(arg).unwrap()
        } else {
            self.call(arg).await
        }
    }
}

new_impls!(async LocalDynAsyncFn, Storage, [for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture], for<'a> AsyncFn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture);

impl_clone!(async LocalDynAsyncFn, Storage);
impl_debug!(async LocalDynAsyncFn, Storage);

/// A dynamic [`AsyncFn`] stored in `FnStorage`, whose returned future is stored in `FutureStorage`.
///
/// Using [`Raw`](crate::storage::Raw)/[`RawOrBox`](crate::storage::RawOrBox) storage avoids the
/// need to allocate the returned future, which results in better performance — `RawOrBox` may
/// fall back to `Box` allocation, but if the returned future is big enough to require it, the
/// allocation cost may be negligible compared to polling it.
///
/// `DynAsyncFn` can also be initialized with a synchronous function, in which case
/// [`call_try_sync`](Self::call_try_sync) offers a lot better performance than
/// [`call`](Self::call).
pub struct DynAsyncFn<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: Storage + StorageSend = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
>(LocalDynAsyncFn<'capture, Arg, Ret, FnStorage, FutureStorage>);

// SAFETY: the object is initialized with a `Send + Sync` function
unsafe_impl_send_sync!(async DynAsyncFn, Storage);

impl<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static,
    FnStorage: Storage + StorageSend,
    FutureStorage: StorageMut,
> DynAsyncFn<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    /// # Safety
    ///
    /// `storage` must have been initialized with `F`.
    const unsafe fn new_impl<F: AsyncFnSend<'capture, Arg, Ret>>(storage: FnStorage) -> Self {
        let vtable = &AsyncVTable {
            call: |func, arg, fut, _| {
                // SAFETY: func comes from `self.storage.ptr()`, so it's a valid `&F`
                store_future(fut, unsafe { func.cast::<F>().as_ref().call(arg) })
            },
            call_sync: None,
            drop_vtable: const { DropVTable::new::<FnStorage, F>() },
        };
        Self(LocalDynAsyncFn {
            // SAFETY: `drop_vtable` matches the storage
            storage: unsafe { DynStorage::new(storage, vtable) },
            _capture: PhantomData,
        })
    }

    /// # Safety
    ///
    /// `storage` must have been initialized with `F`.
    const unsafe fn new_sync_impl<
        F: for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        // SAFETY: same precondition
        Self(unsafe { LocalDynAsyncFn::new_sync_impl::<F>(storage) })
    }

    /// Returns whether the underlying function is synchronous.
    pub fn is_sync(&self) -> bool {
        self.0.is_sync()
    }

    /// Calls the underlying function.
    pub async fn call<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        // SAFETY: Future returned by `AsyncFnSend` implements `Send`,
        // and futures capturing A [`Send`] + [`Sync`] function also implements `Send`
        unsafe { SendFuture::new(self.0.call(arg)).await }
    }

    /// Calls the underlying function if is synchronous.
    pub fn call_sync<'a>(&self, arg: Arg::Of<'a>) -> Option<Ret::Of<'a>> {
        self.0.call_sync(arg)
    }

    /// Tries calling the underlying function as synchronous, falling back to asynchronous call.
    ///
    /// This is equivalent to
    /// ```ignore
    /// if self.is_sync() {
    ///     self.call_sync(arg).unwrap()
    /// } else {
    ///     self.call(arg).await
    /// }
    /// ```
    pub async fn call_try_sync<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        // SAFETY: Future returned by `AsyncFnSend` implements `Send`,
        // and futures capturing A [`Send`] + [`Sync`] function also implements `Send`
        unsafe { SendFuture::new(self.0.call_try_sync(arg)).await }
    }
}

new_impls!(async DynAsyncFn, Storage + StorageSend, [for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture], AsyncFnSend<'capture, Arg, Ret>);

impl_clone!(async DynAsyncFn, Storage + StorageSend);
impl_debug!(async DynAsyncFn, Storage + StorageSend);

/// [`DynAsyncFnMut`], but without the [`Send`] + [`Sync`] requirement.
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
    /// # Safety
    ///
    /// `storage` must have been initialized with `F`.
    const unsafe fn new_impl<
        F: for<'a> AsyncFnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        let vtable = &AsyncVTable {
            call: |func, arg, fut, _| {
                // SAFETY: func comes from `self.storage.ptr_mut()`, so it's a valid `&mut F`
                store_future(fut, unsafe { func.cast::<F>().as_mut()(arg, PhantomData) })
            },
            call_sync: None,
            drop_vtable: const { DropVTable::new::<FnStorage, F>() },
        };
        Self {
            // SAFETY: `drop_vtable` matches the storage
            storage: unsafe { DynStorage::new(storage, vtable) },
            _capture: PhantomData,
        }
    }

    /// # Safety
    ///
    /// `storage` must have been initialized with `F`.
    const unsafe fn new_sync_impl<
        F: for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        let vtable = &AsyncVTable {
            call: |func, arg, fut, _| {
                store_future(fut, async move {
                    // SAFETY: func comes from `self.storage.ptr_mut()`, so it's a valid `&mut F`
                    unsafe { func.cast::<F>().as_mut()(arg, PhantomData) }
                })
            },
            // SAFETY: func comes from `self.storage.ptr_mut()`, so it's a valid `&mut F`
            call_sync: Some(|func, arg, _| unsafe { func.cast::<F>().as_mut()(arg, PhantomData) }),
            drop_vtable: const { DropVTable::new::<FnStorage, F>() },
        };
        Self {
            // SAFETY: `drop_vtable` matches the storage
            storage: unsafe { DynStorage::new(storage, vtable) },
            _capture: PhantomData,
        }
    }

    /// Returns whether the underlying function is synchronous.
    pub fn is_sync(&self) -> bool {
        self.storage.vtable().call_sync.is_some()
    }

    /// Calls the underlying function.
    pub async fn call<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        let mut future = MaybeUninit::uninit();
        let vtable =
            (self.storage.vtable().call)(self.storage.ptr_mut(), arg, &mut future, PhantomData);
        // SAFETY: `future` has been initialized in `call`, and the vtable
        // returned by `store_future` matches the future stored
        unsafe { poll_future(vtable, &mut future) }.await
    }

    /// Calls the underlying function if is synchronous.
    pub fn call_sync<'a>(&mut self, arg: Arg::Of<'a>) -> Option<Ret::Of<'a>> {
        self.storage.vtable().call_sync?(self.storage.ptr_mut(), arg, PhantomData).into()
    }

    /// Tries calling the underlying function as synchronous, falling back to asynchronous call.
    ///
    /// This is equivalent to
    /// ```ignore
    /// if self.is_sync() {
    ///     self.call_sync(arg).unwrap()
    /// } else {
    ///     self.call(arg).await
    /// }
    /// ```
    pub async fn call_try_sync<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        if self.is_sync() {
            self.call_sync(arg).unwrap()
        } else {
            self.call(arg).await
        }
    }
}

new_impls!(async LocalDynAsyncFnMut, StorageMut, [for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture], for<'a> AsyncFnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture);

impl_debug!(async LocalDynAsyncFnMut, StorageMut);

/// A dynamic [`AsyncFnMut`] stored in `FnStorage`, whose returned future is stored in
/// `FutureStorage`.
///
/// Using [`Raw`](crate::storage::Raw)/[`RawOrBox`](crate::storage::RawOrBox) storage avoids the
/// need to allocate the returned future, which results in better performance — `RawOrBox` may
/// fall back to `Box` allocation, but if the returned future is big enough to require it, the
/// allocation cost may be negligible compared to polling it.
///
/// `DynAsyncFnMut` can also be initialized with a synchronous function, in which case
/// [`call_try_sync`](Self::call_try_sync) offers a lot better performance than
/// [`call`](Self::call).
pub struct DynAsyncFnMut<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: StorageMut + StorageSend = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
>(LocalDynAsyncFnMut<'capture, Arg, Ret, FnStorage, FutureStorage>);

unsafe_impl_send_sync!(async DynAsyncFnMut, StorageMut);

impl<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static,
    FnStorage: StorageMut + StorageSend,
    FutureStorage: StorageMut,
> DynAsyncFnMut<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    /// # Safety
    ///
    /// `storage` must have been initialized with `F`.
    const unsafe fn new_impl<F: AsyncFnMutSend<'capture, Arg, Ret>>(storage: FnStorage) -> Self {
        let vtable = &AsyncVTable {
            call: |func, arg, fut, _| {
                // SAFETY: func comes from `self.storage.ptr_mut()`, so it's a valid `&mut F`
                store_future(fut, unsafe { func.cast::<F>().as_mut().call(arg) })
            },
            call_sync: None,
            drop_vtable: const { DropVTable::new::<FnStorage, F>() },
        };
        Self(LocalDynAsyncFnMut {
            // SAFETY: `drop_vtable` matches the storage
            storage: unsafe { DynStorage::new(storage, vtable) },
            _capture: PhantomData,
        })
    }

    /// # Safety
    ///
    /// `storage` must have been initialized with `F`.
    const unsafe fn new_sync_impl<
        F: for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        // SAFETY: same precondition
        Self(unsafe { LocalDynAsyncFnMut::new_sync_impl::<F>(storage) })
    }

    /// Returns whether the underlying function is synchronous.
    pub fn is_sync(&self) -> bool {
        self.0.is_sync()
    }

    /// Calls the underlying function.
    pub async fn call<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        // SAFETY: Future returned by `AsyncFnMutSend` implements `Send`,
        // and futures capturing A [`Send`] + [`Sync`] function also implements `Send`
        unsafe { SendFuture::new(self.0.call(arg)).await }
    }

    /// Calls the underlying function if is synchronous.
    pub fn call_sync<'a>(&mut self, arg: Arg::Of<'a>) -> Option<Ret::Of<'a>> {
        self.0.call_sync(arg)
    }

    /// Tries calling the underlying function as synchronous, falling back to asynchronous call.
    ///
    /// This is equivalent to
    /// ```ignore
    /// if self.is_sync() {
    ///     self.call_sync(arg).unwrap()
    /// } else {
    ///     self.call(arg).await
    /// }
    /// ```
    pub async fn call_try_sync<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        // SAFETY: Future returned by `AsyncFnMutSend` implements `Send`,
        // and futures capturing A [`Send`] + [`Sync`] function also implements `Send`
        unsafe { SendFuture::new(self.0.call_try_sync(arg)).await }
    }
}

new_impls!(async DynAsyncFnMut, StorageMut + StorageSend, [for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture], AsyncFnMutSend<'capture, Arg, Ret>);

impl_debug!(async DynAsyncFnMut, StorageMut + StorageSend);

/// [`DynAsyncFnOnce`], but without the [`Send`] + [`Sync`] requirement.
pub struct LocalDynAsyncFnOnce<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
> {
    storage: DynStorage<FnStorage, AsyncVTable<Arg, Ret, FutureStorage, FnStorage>>,
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
    /// # Safety
    ///
    /// `storage` must have been initialized with `F`.
    const unsafe fn new_impl<
        F: for<'a> AsyncFnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        let vtable = &AsyncVTable {
            call: |func, arg, fut, _| {
                // SAFETY: storage comes from `DynStorage::move_storage`,
                // so it's a valid `F`, and is never accessed after; `read`is called once
                store_future(fut, unsafe {
                    StorageMoved::<FnStorage, F>::new(func).read()(arg, PhantomData)
                })
            },
            call_sync: None,
            drop_vtable: const { DropVTable::new::<FnStorage, F>() },
        };
        Self {
            // SAFETY: `drop_vtable` matches the storage
            storage: unsafe { DynStorage::new(storage, vtable) },
            _capture: PhantomData,
        }
    }

    /// # Safety
    ///
    /// `storage` must have been initialized with `F`.
    const unsafe fn new_sync_impl<
        F: for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        let vtable = &AsyncVTable {
            call: |func, arg, fut, _| {
                store_future(fut, async move {
                    // SAFETY: storage comes from `DynStorage::move_storage`,
                    // so it's a valid `F`, and is never accessed after; `read`is called once
                    unsafe { StorageMoved::<FnStorage, F>::new(func).read()(arg, PhantomData) }
                })
            },
            // SAFETY: storage comes from `DynStorage::move_storage`,
            // so it's a valid `F`, and is never accessed after; `read`is called once
            call_sync: Some(|func, arg, _| unsafe {
                StorageMoved::<FnStorage, F>::new(func).read()(arg, PhantomData)
            }),
            drop_vtable: const { DropVTable::new::<FnStorage, F>() },
        };
        Self {
            // SAFETY: `drop_vtable` matches the storage
            storage: unsafe { DynStorage::new(storage, vtable) },
            _capture: PhantomData,
        }
    }

    /// Returns whether the underlying function is synchronous.
    pub fn is_sync(&self) -> bool {
        self.storage.vtable().call_sync.is_some()
    }

    /// Calls the underlying function.
    pub async fn call<'a>(self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        let mut storage = ManuallyDrop::new(self.storage);
        let mut future = MaybeUninit::uninit();
        // SAFETY: `moved_storage` is passed to `StorageMoved` in `call`
        let moved_storage = unsafe { DynStorage::move_storage(&mut storage) };
        let vtable = (storage.vtable().call)(moved_storage, arg, &mut future, PhantomData);
        // SAFETY: `future` has been initialized in `call`, and the vtable
        // returned by `store_future` matches the future stored
        unsafe { poll_future(vtable, &mut future) }.await
    }

    /// Calls the underlying function if is synchronous.
    pub fn call_sync(self, arg: Arg::Of<'_>) -> Option<Ret::Of<'_>> {
        let call_sync = self.storage.vtable().call_sync?;
        let mut storage = ManuallyDrop::new(self.storage);
        // SAFETY: `moved_storage` is passed to `StorageMoved` in `call_sync`
        let moved_storage = unsafe { DynStorage::move_storage(&mut storage) };
        call_sync(moved_storage, arg, PhantomData).into()
    }

    /// Tries calling the underlying function as synchronous, falling back to asynchronous call.
    ///
    /// This is equivalent to
    /// ```ignore
    /// if self.is_sync() {
    ///     self.call_sync(arg).unwrap()
    /// } else {
    ///     self.call(arg).await
    /// }
    /// ```
    pub async fn call_try_sync<'a>(self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        if self.is_sync() {
            self.call_sync(arg).unwrap()
        } else {
            self.call(arg).await
        }
    }
}

new_impls!(async LocalDynAsyncFnOnce, StorageMut, [for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture], for<'a> AsyncFnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture);

impl_debug!(async LocalDynAsyncFnOnce, StorageMut);

/// A dynamic [`AsyncFnOnce`] stored in `FnStorage`, whose returned future is stored in
/// `FutureStorage`.
///
/// Using [`Raw`](crate::storage::Raw)/[`RawOrBox`](crate::storage::RawOrBox) storage avoids the
/// need to allocate the returned future, which results in better performance — `RawOrBox` may
/// fall back to `Box` allocation, but if the returned future is big enough to require it, the
/// allocation cost may be negligible compared to polling it.
///
/// `DynAsyncFnOnce` can also be initialized with a synchronous function, in which case
/// [`call_try_sync`](Self::call_try_sync) offers a lot better performance than
/// [`call`](Self::call).
pub struct DynAsyncFnOnce<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: StorageMut + StorageSend = DefaultFnStorage,
    FutureStorage: StorageMut = DefaultFutureStorage,
>(LocalDynAsyncFnOnce<'capture, Arg, Ret, FnStorage, FutureStorage>);

unsafe_impl_send_sync!(async DynAsyncFnOnce, StorageMut);

impl<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static,
    FnStorage: StorageMut + StorageSend,
    FutureStorage: StorageMut,
> DynAsyncFnOnce<'capture, Arg, Ret, FnStorage, FutureStorage>
{
    /// # Safety
    ///
    /// `storage` must have been initialized with `F`.
    const unsafe fn new_impl<F: AsyncFnOnceSend<'capture, Arg, Ret>>(storage: FnStorage) -> Self {
        let vtable = &AsyncVTable {
            call: |func, arg, fut, _| {
                // SAFETY: storage comes from `DynStorage::move_storage`,
                // so it's a valid `F`, and is never accessed after; `read`is called once
                store_future(fut, unsafe {
                    StorageMoved::<FnStorage, F>::new(func).read().call(arg)
                })
            },
            call_sync: None,
            drop_vtable: const { DropVTable::new::<FnStorage, F>() },
        };
        Self(LocalDynAsyncFnOnce {
            // SAFETY: `drop_vtable` matches the storage
            storage: unsafe { DynStorage::new(storage, vtable) },
            _capture: PhantomData,
        })
    }

    /// # Safety
    ///
    /// `storage` must have been initialized with `F`.
    const unsafe fn new_sync_impl<
        F: for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        // SAFETY: same precondition
        Self(unsafe { LocalDynAsyncFnOnce::new_sync_impl::<F>(storage) })
    }

    /// Returns whether the underlying function is synchronous.
    pub fn is_sync(&self) -> bool {
        self.0.is_sync()
    }

    /// Calls the underlying function.
    pub async fn call<'a>(self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        // SAFETY: Future returned by `AsyncFnOnceSend` implements `Send`,
        // and futures capturing A [`Send`] + [`Sync`] function also implements `Send`
        unsafe { SendFuture::new(self.0.call(arg)).await }
    }

    /// Calls the underlying function if is synchronous.
    pub fn call_sync(self, arg: Arg::Of<'_>) -> Option<Ret::Of<'_>> {
        self.0.call_sync(arg)
    }

    /// Tries calling the underlying function as synchronous, falling back to asynchronous call.
    ///
    /// This is equivalent to
    /// ```ignore
    /// if self.is_sync() {
    ///     self.call_sync(arg).unwrap()
    /// } else {
    ///     self.call(arg).await
    /// }
    /// ```
    pub async fn call_try_sync<'a>(self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        // SAFETY: Future returned by `AsyncFnOnceSend` implements `Send`,
        // and futures capturing A [`Send`] + [`Sync`] function also implements `Send`
        unsafe { SendFuture::new(self.0.call_try_sync(arg)).await }
    }
}

new_impls!(async DynAsyncFnOnce, StorageMut + StorageSend, [for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture], AsyncFnOnceSend<'capture, Arg, Ret>);

impl_debug!(async DynAsyncFnOnce, StorageMut + StorageSend);
