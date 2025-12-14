use core::{marker::PhantomData, mem::ManuallyDrop, ptr::NonNull};

use higher_kinded_types::{ForFixed, ForLt};

use crate::{
    macros::{impl_debug, new_impls, unsafe_impl_send_sync},
    storage::{DefaultFnStorage, Storage, StorageImpl, StorageMut, StorageOnceImpl},
};

#[expect(type_alias_bounds)]
type CallFn<Arg: ForLt, Ret: ForLt> =
    for<'a, 'b> unsafe fn(NonNull<()>, Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a>;

pub struct LocalDynFn<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: Storage = DefaultFnStorage,
> {
    func: StorageImpl<FnStorage>,
    call: CallFn<Arg, Ret>,
    _capture: PhantomData<&'capture ()>,
}

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: Storage>
    LocalDynFn<'capture, Arg, Ret, FnStorage>
{
    const fn new_impl<F: for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture>(
        func: StorageImpl<FnStorage>,
    ) -> Self {
        Self {
            func,
            call: |func, arg, _| unsafe { func.cast::<F>().as_ref()(arg, PhantomData) },
            _capture: PhantomData,
        }
    }

    pub fn call<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        unsafe { (self.call)(self.func.ptr(), arg, PhantomData) }
    }
}

new_impls!(sync(arc) LocalDynFn, StorageImpl, Storage, for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture);

#[cfg(feature = "alloc")]
impl<'capture, Arg: ForLt, Ret: ForLt> Clone
    for LocalDynFn<'capture, Arg, Ret, crate::storage::Arc>
{
    fn clone(&self) -> Self {
        Self {
            func: self.func.clone(),
            call: self.call,
            _capture: PhantomData,
        }
    }
}

impl_debug!(sync LocalDynFn, Storage);

pub struct DynFn<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: Storage = DefaultFnStorage,
>(LocalDynFn<'capture, Arg, Ret, FnStorage>);

unsafe_impl_send_sync!(sync DynFn, Storage);

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: Storage> DynFn<'capture, Arg, Ret, FnStorage> {
    const fn new_impl<
        F: for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        func: StorageImpl<FnStorage>,
    ) -> Self {
        Self(LocalDynFn::new_impl::<F>(func))
    }

    pub fn call<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        self.0.call(arg)
    }
}
new_impls!(sync(arc) DynFn, StorageImpl, Storage, for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture);

#[cfg(feature = "alloc")]
impl<'capture, Arg: ForLt, Ret: ForLt> Clone for DynFn<'capture, Arg, Ret, crate::storage::Arc> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl_debug!(sync DynFn.0, Storage);

pub struct LocalDynFnMut<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
> {
    func: StorageImpl<FnStorage>,
    call: CallFn<Arg, Ret>,
    _capture: PhantomData<&'capture ()>,
}

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut>
    LocalDynFnMut<'capture, Arg, Ret, FnStorage>
{
    const fn new_impl<
        F: for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        func: StorageImpl<FnStorage>,
    ) -> Self {
        Self {
            func,
            call: |func, arg, _| unsafe { func.cast::<F>().as_mut()(arg, PhantomData) },
            _capture: PhantomData,
        }
    }

    pub fn call<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        unsafe { (self.call)(self.func.ptr_mut(), arg, PhantomData) }
    }
}

new_impls!(sync LocalDynFnMut, StorageImpl, StorageMut, for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture);

impl_debug!(sync LocalDynFnMut, StorageMut);

pub struct DynFnMut<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
>(LocalDynFnMut<'capture, Arg, Ret, FnStorage>);

unsafe_impl_send_sync!(sync DynFnMut, StorageMut);

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut>
    DynFnMut<'capture, Arg, Ret, FnStorage>
{
    const fn new_impl<
        F: for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        func: StorageImpl<FnStorage>,
    ) -> Self {
        Self(LocalDynFnMut::new_impl::<F>(func))
    }

    pub fn call<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        self.0.call(arg)
    }
}

new_impls!(sync DynFnMut, StorageImpl, StorageMut, for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture);

impl_debug!(sync DynFnMut.0, StorageMut);

pub struct LocalDynFnOnce<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
> {
    func: ManuallyDrop<StorageOnceImpl<FnStorage>>,
    call: Option<CallFn<Arg, Ret>>,
    _capture: PhantomData<&'capture ()>,
}

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut> Drop
    for LocalDynFnOnce<'capture, Arg, Ret, FnStorage>
{
    fn drop(&mut self) {
        unsafe { ManuallyDrop::take(&mut self.func) }.drop(self.call.is_none());
    }
}

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut>
    LocalDynFnOnce<'capture, Arg, Ret, FnStorage>
{
    const fn new_impl<
        F: for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        func: StorageOnceImpl<FnStorage>,
    ) -> Self {
        Self {
            func: ManuallyDrop::new(func),
            call: Some(|func, arg, _| unsafe { func.cast::<F>().read()(arg, PhantomData) }),
            _capture: PhantomData,
        }
    }

    pub fn call(mut self, arg: Arg::Of<'_>) -> Ret::Of<'_> {
        let call = unsafe { self.call.take().unwrap_unchecked() };
        unsafe { call(self.func.ptr_once(), arg, PhantomData) }
    }
}

new_impls!(sync LocalDynFnOnce, StorageOnceImpl, StorageMut, for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture);

impl_debug!(sync LocalDynFnOnce, StorageMut);

pub struct DynFnOnce<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
>(LocalDynFnOnce<'capture, Arg, Ret, FnStorage>);

unsafe_impl_send_sync!(sync DynFnOnce, StorageMut);

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut>
    DynFnOnce<'capture, Arg, Ret, FnStorage>
{
    const fn new_impl<
        F: for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        func: StorageOnceImpl<FnStorage>,
    ) -> Self {
        Self(LocalDynFnOnce::new_impl::<F>(func))
    }

    pub fn call(self, arg: Arg::Of<'_>) -> Ret::Of<'_> {
        self.0.call(arg)
    }
}

new_impls!(sync DynFnOnce, StorageOnceImpl, StorageMut, for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture);

impl_debug!(sync DynFnOnce.0, StorageMut);
