use core::{marker::PhantomData, mem::ManuallyDrop, ptr::NonNull};

use higher_kinded_types::{ForFixed, ForLt};

use crate::{
    macros::{impl_clone, impl_debug, new_impls, unsafe_impl_send_sync},
    storage::{
        DefaultFnStorage, DropVTable, DynStorage, Storage, StorageMoved, StorageMut, StorageSend,
        VTable,
    },
};

#[expect(type_alias_bounds)]
type Call<Arg: ForLt, Ret: ForLt, T> =
    for<'a, 'b> fn(NonNull<T>, Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a>;

struct SyncVTable<Arg: ForLt, Ret: ForLt, T = ()> {
    call: Call<Arg, Ret, T>,
    drop_vtable: DropVTable,
}

impl<Arg: ForLt + 'static, Ret: ForLt + 'static, T: 'static> VTable for SyncVTable<Arg, Ret, T> {
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
    /// # Safety
    ///
    /// `storage` must have been initialized with `F`.
    const unsafe fn new_impl<
        F: for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        let vtable = &SyncVTable {
            // SAFETY: func comes from `self.storage.ptr()`, so it's a valid `&F`
            call: |func, arg, _| unsafe { func.cast::<F>().as_ref()(arg, PhantomData) },
            drop_vtable: const { DropVTable::new::<FnStorage, F>() },
        };
        Self {
            // SAFETY: `drop_vtable` matches the storage
            storage: unsafe { DynStorage::new(storage, vtable) },
            _capture: PhantomData,
        }
    }

    pub fn call<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        (self.storage.vtable().call)(self.storage.ptr(), arg, PhantomData)
    }
}

new_impls!(sync LocalDynFn, Storage, for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture);

impl_clone!(sync LocalDynFn, Storage);
impl_debug!(sync LocalDynFn, Storage);

pub struct DynFn<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: Storage + StorageSend = DefaultFnStorage,
>(LocalDynFn<'capture, Arg, Ret, FnStorage>);

unsafe_impl_send_sync!(sync DynFn, Storage);

impl<'capture, Arg: ForLt + 'static, Ret: ForLt + 'static, FnStorage: Storage + StorageSend>
    DynFn<'capture, Arg, Ret, FnStorage>
{
    /// # Safety
    ///
    /// `storage` must have been initialized with `F`.
    const unsafe fn new_impl<
        F: for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        // SAFETY: same precondition
        Self(unsafe { LocalDynFn::new_impl::<F>(storage) })
    }

    pub fn call<'a>(&self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        self.0.call(arg)
    }
}

new_impls!(sync DynFn, Storage + StorageSend, for<'a> Fn(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture);

impl_clone!(sync DynFn, Storage + StorageSend);
impl_debug!(sync DynFn, Storage + StorageSend);

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
    /// # Safety
    ///
    /// `storage` must have been initialized with `F`.
    const unsafe fn new_impl<
        F: for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        let vtable = &SyncVTable {
            // SAFETY: func comes from `self.storage.ptr_mut()`, so it's a valid `&mut F`
            call: |func, arg, _| unsafe { func.cast::<F>().as_mut()(arg, PhantomData) },
            drop_vtable: const { DropVTable::new::<FnStorage, F>() },
        };
        Self {
            // SAFETY: `drop_vtable` matches the storage
            storage: unsafe { DynStorage::new(storage, vtable) },
            _capture: PhantomData,
        }
    }

    pub fn call<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        (self.storage.vtable().call)(self.storage.ptr_mut(), arg, PhantomData)
    }
}

new_impls!(sync LocalDynFnMut, StorageMut, for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture);

impl_debug!(sync LocalDynFnMut, StorageMut);

pub struct DynFnMut<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: StorageMut + StorageSend = DefaultFnStorage,
>(LocalDynFnMut<'capture, Arg, Ret, FnStorage>);

unsafe_impl_send_sync!(sync DynFnMut, StorageMut);

impl<'capture, Arg: ForLt + 'static, Ret: ForLt + 'static, FnStorage: StorageMut + StorageSend>
    DynFnMut<'capture, Arg, Ret, FnStorage>
{
    /// # Safety
    ///
    /// `storage` must have been initialized with `F`.
    const unsafe fn new_impl<
        F: for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        // SAFETY: same precondition
        Self(unsafe { LocalDynFnMut::new_impl::<F>(storage) })
    }

    pub fn call<'a>(&mut self, arg: Arg::Of<'a>) -> Ret::Of<'a> {
        self.0.call(arg)
    }
}

new_impls!(sync DynFnMut, StorageMut + StorageSend, for<'a> FnMut(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture);

impl_debug!(sync DynFnMut, StorageMut + StorageSend);

pub struct LocalDynFnOnce<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: StorageMut = DefaultFnStorage,
> {
    storage: DynStorage<FnStorage, SyncVTable<Arg, Ret, FnStorage>>,
    _capture: PhantomData<&'capture ()>,
}

impl<'capture, Arg: ForLt + 'static, Ret: ForLt + 'static, FnStorage: StorageMut>
    LocalDynFnOnce<'capture, Arg, Ret, FnStorage>
{
    /// # Safety
    ///
    /// `storage` must have been initialized with `F`.
    const unsafe fn new_impl<
        F: for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        let vtable = &SyncVTable {
            // SAFETY: storage comes from `DynStorage::move_storage`,
            // so it's a valid `F`, and is never accessed after; `read`is called once
            call: |storage, arg, _| unsafe {
                StorageMoved::<FnStorage, F>::new(storage).read()(arg, PhantomData)
            },
            drop_vtable: const { DropVTable::new::<FnStorage, F>() },
        };
        Self {
            // SAFETY: `drop_vtable` matches the storage
            storage: unsafe { DynStorage::new(storage, vtable) },
            _capture: PhantomData,
        }
    }

    pub fn call(self, arg: Arg::Of<'_>) -> Ret::Of<'_> {
        let mut storage = ManuallyDrop::new(self.storage);
        // SAFETY: `moved_storage` is passed to `StorageMoved` in `call`
        let moved_storage = unsafe { DynStorage::move_storage(&mut storage) };
        (storage.vtable().call)(moved_storage, arg, PhantomData)
    }
}

new_impls!(sync LocalDynFnOnce, StorageMut, for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + 'capture);

impl_debug!(sync LocalDynFnOnce, StorageMut);

pub struct DynFnOnce<
    'capture,
    Arg: ForLt + 'static,
    Ret: ForLt + 'static = ForFixed<()>,
    FnStorage: StorageMut + StorageSend = DefaultFnStorage,
>(LocalDynFnOnce<'capture, Arg, Ret, FnStorage>);

unsafe_impl_send_sync!(sync DynFnOnce, StorageMut);

impl<'capture, Arg: ForLt + 'static, Ret: ForLt + 'static, FnStorage: StorageMut + StorageSend>
    DynFnOnce<'capture, Arg, Ret, FnStorage>
{
    /// # Safety
    ///
    /// `storage` must have been initialized with `F`.
    const unsafe fn new_impl<
        F: for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture,
    >(
        storage: FnStorage,
    ) -> Self {
        // SAFETY: same precondition
        Self(unsafe { LocalDynFnOnce::new_impl::<F>(storage) })
    }

    pub fn call(self, arg: Arg::Of<'_>) -> Ret::Of<'_> {
        self.0.call(arg)
    }
}

new_impls!(sync DynFnOnce, StorageMut + StorageSend, for<'a> FnOnce(Arg::Of<'a>, PhantomData<&'a ()>) -> Ret::Of<'a> + Send + Sync + 'capture);

impl_debug!(sync DynFnOnce, StorageMut + StorageSend);
