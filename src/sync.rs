use core::{marker::PhantomData, mem::ManuallyDrop, ptr::NonNull};

use higher_kinded_types::{ForFixed, ForLt};

use crate::{
    macros::{impl_debug, new_impls, unsafe_impl_send_sync},
    storage::{
        DefaultFnStorage, DropVTable, DynStorage, Storage, StorageMoved, StorageMut, VTable,
    },
};

#[expect(type_alias_bounds)]
type Call<Arg: ForLt, Ret: ForLt> =
    for<'a, 'b> unsafe fn(NonNull<()>, Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a>;

struct SyncVTable<Arg: ForLt, Ret: ForLt> {
    call: Call<Arg, Ret>,
    drop_vtable: DropVTable,
}

impl<Arg: ForLt + 'static, Ret: ForLt + 'static> VTable for SyncVTable<Arg, Ret> {
    fn drop_vtable(&self) -> &DropVTable {
        &self.drop_vtable
    }
}

pub struct LocalDynFn<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: Storage = DefaultFnStorage,
> {
    storage: DynStorage<FnStorage, SyncVTable<Arg, Ret>>,
    _capture: PhantomData<&'capture ()>,
}

impl<'capture, Arg: ForLt + 'static, Ret: ForLt + 'static, FnStorage: Storage>
    LocalDynFn<'capture, Arg, Ret, FnStorage>
{
    const fn new_impl<F: for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture>(
        storage: FnStorage,
    ) -> Self {
        let vtable = &SyncVTable {
            call: |func, arg, _| unsafe { func.cast::<F>().as_ref()(arg, PhantomData) },
            drop_vtable: const { DropVTable::new::<FnStorage, F>() },
        };
        Self {
            storage: DynStorage { storage, vtable },
            _capture: PhantomData,
        }
    }

    pub fn call<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        unsafe { (self.storage.vtable.call)(self.storage.ptr(), arg, PhantomData) }
    }
}

new_impls!(sync(arc) LocalDynFn, Storage, for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture);

#[cfg(feature = "alloc")]
impl<'capture, Arg: ForLt + 'static, Ret: ForLt> Clone
    for LocalDynFn<'capture, Arg, Ret, crate::storage::Arc>
{
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            _capture: PhantomData,
        }
    }
}

impl_debug!(sync LocalDynFn, Storage);

pub struct DynFn<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: Storage = DefaultFnStorage,
>(LocalDynFn<'capture, Arg, Ret, FnStorage>);

unsafe_impl_send_sync!(sync DynFn, Storage);

impl<'capture, Arg: ForLt + 'static, Ret: ForLt + 'static, FnStorage: Storage>
    DynFn<'capture, Arg, Ret, FnStorage>
{
    const fn new_impl<
        F: for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        Self(LocalDynFn::new_impl::<F>(storage))
    }

    pub fn call<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        self.0.call(arg)
    }
}
new_impls!(sync(arc) DynFn, Storage, for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture);

#[cfg(feature = "alloc")]
impl<'capture, Arg: ForLt + 'static, Ret: ForLt> Clone
    for DynFn<'capture, Arg, Ret, crate::storage::Arc>
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl_debug!(sync DynFn, Storage);

pub struct LocalDynFnMut<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
> {
    storage: DynStorage<FnStorage, SyncVTable<Arg, Ret>>,
    _capture: PhantomData<&'capture ()>,
}

impl<'capture, Arg: ForLt + 'static, Ret: ForLt + 'static, FnStorage: StorageMut>
    LocalDynFnMut<'capture, Arg, Ret, FnStorage>
{
    const fn new_impl<
        F: for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        let vtable = &SyncVTable {
            call: |func, arg, _| unsafe { func.cast::<F>().as_mut()(arg, PhantomData) },
            drop_vtable: const { DropVTable::new::<FnStorage, F>() },
        };
        Self {
            storage: DynStorage { storage, vtable },
            _capture: PhantomData,
        }
    }

    pub fn call<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        unsafe { (self.storage.vtable.call)(self.storage.ptr_mut(), arg, PhantomData) }
    }
}

new_impls!(sync LocalDynFnMut, StorageMut, for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture);

impl_debug!(sync LocalDynFnMut, StorageMut);

pub struct DynFnMut<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
>(LocalDynFnMut<'capture, Arg, Ret, FnStorage>);

unsafe_impl_send_sync!(sync DynFnMut, StorageMut);

impl<'capture, Arg: ForLt + 'static, Ret: ForLt + 'static, FnStorage: StorageMut>
    DynFnMut<'capture, Arg, Ret, FnStorage>
{
    const fn new_impl<
        F: for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        Self(LocalDynFnMut::new_impl::<F>(storage))
    }

    pub fn call<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        self.0.call(arg)
    }
}

new_impls!(sync DynFnMut, StorageMut, for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture);

impl_debug!(sync DynFnMut, StorageMut);

pub struct LocalDynFnOnce<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
> {
    storage: DynStorage<FnStorage, SyncVTable<Arg, Ret>>,
    _capture: PhantomData<&'capture ()>,
}

impl<'capture, Arg: ForLt + 'static, Ret: ForLt + 'static, FnStorage: StorageMut>
    LocalDynFnOnce<'capture, Arg, Ret, FnStorage>
{
    const fn new_impl<
        F: for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        let vtable = &SyncVTable {
            call: |func, arg, _| unsafe {
                StorageMoved::<FnStorage, F>::new(func).read()(arg, PhantomData)
            },
            drop_vtable: const { DropVTable::new::<FnStorage, F>() },
        };
        Self {
            storage: DynStorage { storage, vtable },
            _capture: PhantomData,
        }
    }

    pub fn call(self, arg: Arg::Of<'_>) -> Ret::Of<'_> {
        let mut storage = ManuallyDrop::new(self.storage);
        unsafe { (storage.vtable.call)(storage.ptr_mut(), arg, PhantomData) }
    }
}

new_impls!(sync LocalDynFnOnce, StorageMut, for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture);

impl_debug!(sync LocalDynFnOnce, StorageMut);

pub struct DynFnOnce<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
>(LocalDynFnOnce<'capture, Arg, Ret, FnStorage>);

unsafe_impl_send_sync!(sync DynFnOnce, StorageMut);

impl<'capture, Arg: ForLt + 'static, Ret: ForLt + 'static, FnStorage: StorageMut>
    DynFnOnce<'capture, Arg, Ret, FnStorage>
{
    const fn new_impl<
        F: for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        Self(LocalDynFnOnce::new_impl::<F>(storage))
    }

    pub fn call(self, arg: Arg::Of<'_>) -> Ret::Of<'_> {
        self.0.call(arg)
    }
}

new_impls!(sync DynFnOnce, StorageMut, for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture);

impl_debug!(sync DynFnOnce, StorageMut);
