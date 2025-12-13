use core::{marker::PhantomData, mem::ManuallyDrop, ptr::NonNull};

use higher_kinded_types::{ForFixed, ForLt};

use crate::{
    impl_debug,
    storage::{DefaultFnStorage, Storage, StorageImpl, StorageMut, StorageOnceImpl},
};

#[expect(type_alias_bounds)]
type CallFn<Arg: ForLt, Ret: ForLt> =
    for<'a, 'b> unsafe fn(NonNull<()>, Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a>;

struct DynFnImpl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: Storage, const LOCAL: bool> {
    func: StorageImpl<FnStorage>,
    call: CallFn<Arg, Ret>,
    _capture: PhantomData<&'capture ()>,
}

unsafe impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: Storage> Send
    for DynFnImpl<'capture, Arg, Ret, FnStorage, false>
{
}
unsafe impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: Storage> Sync
    for DynFnImpl<'capture, Arg, Ret, FnStorage, false>
{
}

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: Storage, const LOCAL: bool>
    DynFnImpl<'capture, Arg, Ret, FnStorage, LOCAL>
{
    fn new<F: for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture>(
        f: F,
    ) -> Self {
        Self {
            func: StorageImpl::new(f),
            call: |func, arg, _| unsafe { func.cast::<F>().as_ref()(arg, PhantomData) },
            _capture: PhantomData,
        }
    }

    fn new_mut<F: for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture>(
        f: F,
    ) -> Self {
        Self {
            func: StorageImpl::new(f),
            call: |func, arg, _| unsafe { func.cast::<F>().as_mut()(arg, PhantomData) },
            _capture: PhantomData,
        }
    }

    pub unsafe fn call<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        unsafe { (self.call)(self.func.ptr(), arg, PhantomData) }
    }

    pub unsafe fn call_mut<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        unsafe { (self.call)(self.func.ptr_mut(), arg, PhantomData) }
    }
}

pub struct DynFn<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: Storage = DefaultFnStorage,
>(DynFnImpl<'capture, Arg, Ret, FnStorage, false>);

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: Storage> DynFn<'capture, Arg, Ret, FnStorage> {
    pub fn new<
        F: for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        f: F,
    ) -> Self {
        Self(DynFnImpl::new(f))
    }

    pub fn call<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        unsafe { self.0.call(arg) }
    }
}

#[cfg(feature = "alloc")]
impl<'capture, Arg: ForLt, Ret: ForLt> Clone for DynFn<'capture, Arg, Ret, crate::storage::Arc> {
    fn clone(&self) -> Self {
        Self(DynFnImpl {
            func: self.0.func.clone(),
            call: self.0.call,
            _capture: PhantomData,
        })
    }
}

impl_debug!(sync DynFn, Storage);

pub struct LocalDynFn<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: Storage = DefaultFnStorage,
>(DynFnImpl<'capture, Arg, Ret, FnStorage, true>);

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: Storage>
    LocalDynFn<'capture, Arg, Ret, FnStorage>
{
    pub fn new<F: for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture>(
        f: F,
    ) -> Self {
        Self(DynFnImpl::new(f))
    }

    pub fn call<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        unsafe { self.0.call(arg) }
    }
}

#[cfg(feature = "alloc")]
impl<'capture, Arg: ForLt, Ret: ForLt> Clone
    for LocalDynFn<'capture, Arg, Ret, crate::storage::Arc>
{
    fn clone(&self) -> Self {
        Self(DynFnImpl {
            func: self.0.func.clone(),
            call: self.0.call,
            _capture: PhantomData,
        })
    }
}

impl_debug!(sync LocalDynFn, Storage);

pub struct DynFnMut<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
>(DynFnImpl<'capture, Arg, Ret, FnStorage, false>);

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut>
    DynFnMut<'capture, Arg, Ret, FnStorage>
{
    pub fn new<
        F: for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        f: F,
    ) -> Self {
        Self(DynFnImpl::new_mut(f))
    }

    pub fn call<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        unsafe { self.0.call_mut(arg) }
    }
}

impl_debug!(sync DynFnMut, StorageMut);

pub struct LocalDynFnMut<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
>(DynFnImpl<'capture, Arg, Ret, FnStorage, true>);

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut>
    LocalDynFnMut<'capture, Arg, Ret, FnStorage>
{
    pub fn new<F: for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture>(
        f: F,
    ) -> Self {
        Self(DynFnImpl::new_mut(f))
    }

    pub fn call<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        unsafe { self.0.call_mut(arg) }
    }
}

impl_debug!(sync LocalDynFnMut, StorageMut);

struct DynFnOnceImpl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut, const LOCAL: bool> {
    func: ManuallyDrop<StorageOnceImpl<FnStorage>>,
    call: Option<CallFn<Arg, Ret>>,
    _capture: PhantomData<&'capture ()>,
}

unsafe impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut> Send
    for DynFnOnceImpl<'capture, Arg, Ret, FnStorage, false>
{
}
unsafe impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut> Sync
    for DynFnOnceImpl<'capture, Arg, Ret, FnStorage, false>
{
}

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut, const LOCAL: bool> Drop
    for DynFnOnceImpl<'capture, Arg, Ret, FnStorage, LOCAL>
{
    fn drop(&mut self) {
        unsafe { ManuallyDrop::take(&mut self.func) }.drop(self.call.is_none());
    }
}

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut, const LOCAL: bool>
    DynFnOnceImpl<'capture, Arg, Ret, FnStorage, LOCAL>
{
    fn new<F: for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture>(
        f: F,
    ) -> Self {
        Self {
            func: ManuallyDrop::new(StorageOnceImpl::new(f)),
            call: Some(|func, arg, _| unsafe { func.cast::<F>().read()(arg, PhantomData) }),
            _capture: PhantomData,
        }
    }

    pub fn call(mut self, arg: Arg::Of<'_>) -> Ret::Of<'_> {
        let call = unsafe { self.call.take().unwrap_unchecked() };
        unsafe { call(self.func.ptr_once(), arg, PhantomData) }
    }
}

pub struct DynFnOnce<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
>(DynFnOnceImpl<'capture, Arg, Ret, FnStorage, false>);

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut>
    DynFnOnce<'capture, Arg, Ret, FnStorage>
{
    pub fn new<
        F: for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        f: F,
    ) -> Self {
        Self(DynFnOnceImpl::new(f))
    }

    pub fn call(self, arg: Arg::Of<'_>) -> Ret::Of<'_> {
        self.0.call(arg)
    }
}

impl_debug!(sync DynFnOnce, StorageMut);

pub struct LocalDynFnOnce<
    'capture,
    Arg: ForLt,
    Ret: ForLt = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
>(DynFnOnceImpl<'capture, Arg, Ret, FnStorage, true>);

impl<'capture, Arg: ForLt, Ret: ForLt, FnStorage: StorageMut>
    LocalDynFnOnce<'capture, Arg, Ret, FnStorage>
{
    pub fn new<F: for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture>(
        f: F,
    ) -> Self {
        Self(DynFnOnceImpl::new(f))
    }

    pub fn call<'a>(self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        self.0.call(arg)
    }
}

impl_debug!(sync LocalDynFnOnce, StorageMut);
