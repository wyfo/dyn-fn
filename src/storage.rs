//! The storages available for the dynamic functions/returned futures.

#[cfg(feature = "alloc")]
use alloc::{boxed::Box as StdBox, rc::Rc as StdRc, sync::Arc as StdArc};
use core::{
    alloc::Layout,
    marker::{PhantomData, PhantomPinned},
    mem,
    mem::{ManuallyDrop, MaybeUninit},
    ptr::NonNull,
};

use elain::{Align, Alignment};

#[cfg(not(feature = "alloc"))]
/// Default function storage.
pub type DefaultFnStorage = Raw<{ size_of::<usize>() }>;
#[cfg(feature = "alloc")]
/// Default function storage.
pub type DefaultFnStorage = Box;
/// Default future storage.
pub type DefaultFutureStorage = RawOrBox<{ 16 * size_of::<usize>() }>;

/// A storage that can be used to store dynamic type-erased objects.
pub trait Storage: private::Storage {}
/// A [`storage`] whose mutable access gives mutable access to the stored object.
pub trait StorageMut: Storage {}
/// A storage implementing [`Send`] + [`Sync`] if the stored object implements [`Send`] + [`Sync`].
pub trait StorageSend: private::StorageSend {}

pub(crate) struct DropVTable {
    /// # Safety
    ///
    /// See [`private::Storage::drop_inner`].
    drop_inner: Option<unsafe fn(NonNull<()>)>,
    layout: Layout,
}

impl DropVTable {
    #[cfg_attr(coverage_nightly, coverage(off))] // const fn
    pub(crate) const fn new<S: Storage, T>() -> Self {
        Self {
            drop_inner: const {
                if S::NEEDS_DROP_INNER || mem::needs_drop::<T>() {
                    Some(S::drop_inner::<T>)
                } else {
                    None
                }
            },
            layout: const { Layout::new::<T>() },
        }
    }

    /// # Safety
    ///
    /// The vtable must match the data stored in the storage,
    /// and the storage must be accessed after the call.
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) unsafe fn drop_storage(&self, storage: &mut impl Storage) {
        if let Some(drop_inner) = self.drop_inner {
            // SAFETY: the storage data is no longer accessed after the call,
            // and is matched by the vtable as per function contract.
            unsafe { drop_inner(storage.ptr_mut()) };
        }
        // SAFETY: the storage data is no longer accessed after the call,
        // and is matched by the vtable as per function contract.
        unsafe { storage.drop_in_place(self.layout) };
    }
}

pub(crate) trait VTable: 'static {
    fn drop_vtable(&self) -> &DropVTable;
}

#[derive(Debug)]
pub(crate) struct DynStorage<S: Storage, VT: VTable> {
    storage: S,
    vtable: &'static VT,
}

impl<S: Storage, VT: VTable> DynStorage<S, VT> {
    /// # Safety
    ///
    /// `vtable.drop_vtable()` must match the data stored in `storage`.
    pub(crate) const unsafe fn new(storage: S, vtable: &'static VT) -> Self {
        Self { storage, vtable }
    }

    pub(crate) fn vtable(&self) -> &'static VT {
        self.vtable
    }

    /// # Safety
    ///
    /// The returned storage must be used only to instantiate `StorageMoved`.
    pub(crate) unsafe fn move_storage(this: &mut ManuallyDrop<Self>) -> NonNull<S> {
        (&mut this.storage).into()
    }

    pub(crate) fn ptr<T>(&self) -> NonNull<T> {
        self.storage.ptr().cast()
    }

    pub(crate) fn ptr_mut<T>(&mut self) -> NonNull<T> {
        self.storage.ptr_mut().cast()
    }
}

impl<S: Storage, VT: VTable> Drop for DynStorage<S, VT> {
    fn drop(&mut self) {
        // SAFETY: `Self::new` ensures the vtable matches the data stored;
        // the storage is no longer accessed after the call (because it's dropped)
        unsafe { self.vtable.drop_vtable().drop_storage(&mut self.storage) }
    }
}

#[cfg(feature = "alloc")]
impl<S: Storage + Clone, VT: VTable> Clone for DynStorage<S, VT> {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            vtable: self.vtable,
        }
    }
}

pub(crate) struct StorageMoved<S: StorageMut, T> {
    storage: NonNull<S>,
    _phantom: PhantomData<T>,
}

impl<S: StorageMut, T> StorageMoved<S, T> {
    /// # Safety
    ///
    /// `storage` must have been instantiated with type `T`.
    /// `storage` must neither be accessed, nor dropped, after `StorageMoved` instantiation.
    pub(crate) unsafe fn new(storage: NonNull<S>) -> Self {
        Self {
            storage,
            _phantom: PhantomData,
        }
    }

    /// # Safety
    ///
    /// `read` must be called only once.
    pub(crate) unsafe fn read(&self) -> T {
        // SAFETY: `storage` stores a `T`
        unsafe { self.storage.as_ref().ptr().cast().read() }
    }
}

impl<S: StorageMut, T> Drop for StorageMoved<S, T> {
    fn drop(&mut self) {
        // SAFETY: the storage data is no longer accessed after the call,
        // and is matched by the vtable as per function contract, as per
        // `Self::new` contract
        unsafe { self.storage.as_mut().drop_in_place(Layout::new::<T>()) }
    }
}

/// A raw storage, where object is stored in place.
///
/// Object size and alignment must fit, e.g. be lesser or equal to the generic parameter.
/// This condition is enforced by a constant assertion, which triggers at build time
/// (it is not triggered by **cargo check**).
#[derive(Debug)]
#[repr(C)]
pub struct Raw<const SIZE: usize, const ALIGN: usize = { align_of::<usize>() }>
where
    Align<ALIGN>: Alignment,
{
    data: MaybeUninit<[u8; SIZE]>,
    _align: Align<ALIGN>,
    _not_send_sync: PhantomData<*mut ()>,
    _pinned: PhantomPinned,
}

impl<const SIZE: usize, const ALIGN: usize> Raw<SIZE, ALIGN>
where
    Align<ALIGN>: Alignment,
{
    /// # Safety
    ///
    /// `data` must have size and alignment lesser or equal to the generic parameters.
    const unsafe fn new_unchecked<T>(data: T) -> Self {
        let mut raw = Self {
            data: MaybeUninit::uninit(),
            _align: Align::NEW,
            _not_send_sync: PhantomData,
            _pinned: PhantomPinned,
        };
        // SAFETY: function contract guarantees that `raw.data` size and alignment
        // matches `data` ones; alignment is obtained through `_align` field and `repr(C)`
        unsafe { raw.data.as_mut_ptr().cast::<T>().write(data) };
        raw
    }

    pub(crate) const fn new<T>(data: T) -> Self {
        const { assert!(size_of::<T>() <= SIZE) };
        const { assert!(align_of::<T>() <= ALIGN) };
        // SAFETY: assertion above ensures function contract
        unsafe { Self::new_unchecked::<T>(data) }
    }
}

impl<const SIZE: usize, const ALIGN: usize> Storage for Raw<SIZE, ALIGN> where
    Align<ALIGN>: Alignment
{
}
impl<const SIZE: usize, const ALIGN: usize> StorageMut for Raw<SIZE, ALIGN> where
    Align<ALIGN>: Alignment
{
}
impl<const SIZE: usize, const ALIGN: usize> StorageSend for Raw<SIZE, ALIGN> where
    Align<ALIGN>: Alignment
{
}

/// A type-erased [`Box`](StdBox).
#[cfg(feature = "alloc")]
#[derive(Debug)]
pub struct Box(NonNull<()>);
#[cfg(feature = "alloc")]
impl Box {
    pub(crate) fn new_box<T>(data: StdBox<T>) -> Self {
        Self(NonNull::new(StdBox::into_raw(data).cast()).unwrap())
    }
}
#[cfg(feature = "alloc")]
impl Storage for Box {}
#[cfg(feature = "alloc")]
impl StorageMut for Box {}
#[cfg(feature = "alloc")]
impl StorageSend for Box {}

/// A type-erased [`Rc`](StdRc).
#[cfg(feature = "alloc")]
#[derive(Debug)]
pub struct Rc(NonNull<()>);
#[cfg(feature = "alloc")]
impl Rc {
    pub(crate) fn new_rc<T>(data: StdRc<T>) -> Self {
        Self(NonNull::new(StdRc::into_raw(data).cast_mut().cast()).unwrap())
    }
}
#[cfg(feature = "alloc")]
impl Clone for Rc {
    fn clone(&self) -> Self {
        // SAFETY: The pointer has been obtained through `Rc::into_raw`,
        // and the `Rc` instance is still valid because strong
        // count is only decremented in drop.
        unsafe { StdRc::increment_strong_count(self.0.as_ptr()) };
        Self(self.0)
    }
}
#[cfg(feature = "alloc")]
impl Storage for Rc {}

/// A type-erased [`Arc`](StdArc).
#[cfg(feature = "alloc")]
#[derive(Debug)]
pub struct Arc(NonNull<()>);
#[cfg(feature = "alloc")]
impl Arc {
    pub(crate) fn new_arc<T>(data: StdArc<T>) -> Self {
        Self(NonNull::new(StdArc::into_raw(data).cast_mut().cast()).unwrap())
    }
}
#[cfg(feature = "alloc")]
impl Clone for Arc {
    fn clone(&self) -> Self {
        // SAFETY: The pointer has been obtained through `Arc::into_raw`,
        // and the `Arc` instance is still valid because strong
        // count is only decremented in drop.
        unsafe { StdArc::increment_strong_count(self.0.as_ptr()) };
        Self(self.0)
    }
}
#[cfg(feature = "alloc")]
impl Storage for Arc {}
#[cfg(feature = "alloc")]
impl StorageSend for Arc {}

#[derive(Debug)]
enum RawOrBoxInner<const SIZE: usize, const ALIGN: usize = { align_of::<usize>() }>
where
    Align<ALIGN>: Alignment,
{
    Raw(Raw<SIZE, ALIGN>),
    #[cfg(feature = "alloc")]
    Box(Box),
}

/// A [`Raw`] storage with [`Box`] backup if the object doesn't fit in.
#[derive(Debug)]
pub struct RawOrBox<const SIZE: usize, const ALIGN: usize = { align_of::<usize>() }>(
    RawOrBoxInner<SIZE, ALIGN>,
)
where
    Align<ALIGN>: Alignment;

#[cfg(feature = "alloc")]
impl<const SIZE: usize, const ALIGN: usize> RawOrBox<SIZE, ALIGN>
where
    Align<ALIGN>: Alignment,
{
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) const fn new_raw<T>(data: T) -> Self {
        Self(RawOrBoxInner::Raw(Raw::new(data)))
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn new_box<T>(data: StdBox<T>) -> Self {
        Self(RawOrBoxInner::Box(Box::new_box(data)))
    }
}

impl<const SIZE: usize, const ALIGN: usize> Storage for RawOrBox<SIZE, ALIGN> where
    Align<ALIGN>: Alignment
{
}
impl<const SIZE: usize, const ALIGN: usize> StorageMut for RawOrBox<SIZE, ALIGN> where
    Align<ALIGN>: Alignment
{
}
impl<const SIZE: usize, const ALIGN: usize> StorageSend for RawOrBox<SIZE, ALIGN> where
    Align<ALIGN>: Alignment
{
}

pub(crate) mod private {
    #[cfg(feature = "alloc")]
    use alloc::{boxed::Box, rc::Rc, sync::Arc};
    use core::{alloc::Layout, ptr::NonNull};

    use elain::{Align, Alignment};

    /// # Safety
    ///
    /// `ptr`/`ptr_mut` must return a pointer to the data stored in the storage.
    pub unsafe trait Storage: Sized + 'static {
        const NEEDS_DROP_INNER: bool = false;
        fn new<T>(data: T) -> Self;
        fn ptr(&self) -> NonNull<()>;
        fn ptr_mut(&mut self) -> NonNull<()>;
        /// # Safety
        ///
        /// `ptr_mut` must have been obtained from `Storage::ptr_mut`.
        /// Storage must have been instantiated with a data of type `T`.
        /// Storage data must not be accessed after calling this method.
        unsafe fn drop_inner<T>(ptr_mut: NonNull<()>) {
            // SAFETY: `ptr_mut` is a pointer to `T` as per function and trait contracts,
            // and is no longer accessed after the call.
            unsafe { ptr_mut.cast::<T>().drop_in_place() }
        }
        /// # Safety
        ///
        /// `drop_in_place` must be called once, and the storage must not be used
        /// after. `layout` must be the layout of the `data` passed in `Self::new`
        /// (or in other constructor like `new_box`, etc.)
        unsafe fn drop_in_place(&mut self, layout: Layout);
    }

    /// # Safety
    ///
    /// The underlying storage must implement `Send` + `Sync` if the stored data
    /// implements `Send` + `Sync`.
    pub unsafe trait StorageSend {}

    // SAFETY: `ptr`/`ptr_mut` return a pointer to the stored data.
    unsafe impl<const SIZE: usize, const ALIGN: usize> Storage for super::Raw<SIZE, ALIGN>
    where
        Align<ALIGN>: Alignment,
    {
        fn new<T>(data: T) -> Self {
            Self::new(data)
        }
        fn ptr(&self) -> NonNull<()> {
            NonNull::from(&self.data).cast()
        }
        fn ptr_mut(&mut self) -> NonNull<()> {
            NonNull::from(&mut self.data).cast()
        }
        unsafe fn drop_in_place(&mut self, _layout: Layout) {}
    }

    // SAFETY: Raw storage has the same guarantee the data it stores.
    unsafe impl<const SIZE: usize, const ALIGN: usize> StorageSend for super::Raw<SIZE, ALIGN> where
        Align<ALIGN>: Alignment
    {
    }

    // SAFETY: `ptr`/`ptr_mut` return a pointer to the stored data.
    #[cfg(feature = "alloc")]
    unsafe impl Storage for super::Box {
        fn new<T>(data: T) -> Self {
            Self::new_box(Box::new(data))
        }
        fn ptr(&self) -> NonNull<()> {
            self.0
        }
        fn ptr_mut(&mut self) -> NonNull<()> {
            self.0
        }
        unsafe fn drop_in_place(&mut self, layout: Layout) {
            if layout.size() != 0 {
                // SAFETY: storage has been initialized with `Box<T>`,
                // and `layout` must be `Layout::new::<T>()` as per function contract
                unsafe { alloc::alloc::dealloc(self.0.as_ptr().cast(), layout) };
            }
        }
    }
    // SAFETY: Box has the same guarantee the data it stores.
    #[cfg(feature = "alloc")]
    unsafe impl StorageSend for super::Box {}

    // SAFETY: `ptr`/`ptr_mut` return a pointer to the stored data.
    #[cfg(feature = "alloc")]
    unsafe impl Storage for super::Rc {
        const NEEDS_DROP_INNER: bool = true;
        fn new<T>(data: T) -> Self {
            Self::new_rc(Rc::new(data))
        }
        fn ptr(&self) -> NonNull<()> {
            self.0
        }
        fn ptr_mut(&mut self) -> NonNull<()> {
            self.0
        }
        unsafe fn drop_inner<T>(ptr_mut: NonNull<()>) {
            // SAFETY: storage has been initialized with `Rc<T>`
            drop(unsafe { Rc::<T>::from_raw(ptr_mut.cast().as_ptr()) });
        }
        unsafe fn drop_in_place(&mut self, _layout: Layout) {}
    }

    // SAFETY: `ptr`/`ptr_mut` return a pointer to the stored data.
    #[cfg(feature = "alloc")]
    unsafe impl Storage for super::Arc {
        const NEEDS_DROP_INNER: bool = true;
        fn new<T>(data: T) -> Self {
            Self::new_arc(Arc::new(data))
        }
        fn ptr(&self) -> NonNull<()> {
            self.0
        }
        fn ptr_mut(&mut self) -> NonNull<()> {
            self.0
        }
        unsafe fn drop_inner<T>(ptr_mut: NonNull<()>) {
            // SAFETY: storage has been initialized with `Arc<T>`
            drop(unsafe { Arc::<T>::from_raw(ptr_mut.cast().as_ptr()) });
        }
        unsafe fn drop_in_place(&mut self, _layout: Layout) {}
    }

    // SAFETY: `Arc` implements `Send` + `Sync` when data implements `Send` + `Sync`
    #[cfg(feature = "alloc")]
    unsafe impl StorageSend for super::Arc {}

    // SAFETY: Both `Raw` and `Box` implements `Storage`
    // This enum is generic and the variant is chosen according constant predicate,
    // so it's not possible to cover all variant for a specific monomorphization.
    // https://github.com/taiki-e/cargo-llvm-cov/issues/394
    #[cfg_attr(coverage_nightly, coverage(off))]
    unsafe impl<const SIZE: usize, const ALIGN: usize> Storage for super::RawOrBox<SIZE, ALIGN>
    where
        Align<ALIGN>: Alignment,
    {
        fn new<T>(data: T) -> Self {
            #[cfg(feature = "alloc")]
            if size_of::<T>() <= SIZE && align_of::<T>() <= ALIGN {
                // SAFETY: size and alignment are checked above
                Self(super::RawOrBoxInner::Raw(unsafe {
                    super::Raw::new_unchecked(data)
                }))
            } else {
                Self(super::RawOrBoxInner::Box(super::Box::new(data)))
            }
            #[cfg(not(feature = "alloc"))]
            {
                Self(super::RawOrBoxInner::Raw(super::Raw::new(data)))
            }
        }
        fn ptr(&self) -> NonNull<()> {
            match &self.0 {
                super::RawOrBoxInner::Raw(s) => s.ptr(),
                #[cfg(feature = "alloc")]
                super::RawOrBoxInner::Box(s) => s.ptr(),
            }
        }
        fn ptr_mut(&mut self) -> NonNull<()> {
            match &mut self.0 {
                super::RawOrBoxInner::Raw(s) => s.ptr_mut(),
                #[cfg(feature = "alloc")]
                super::RawOrBoxInner::Box(s) => s.ptr_mut(),
            }
        }
        unsafe fn drop_in_place(&mut self, layout: Layout) {
            match &mut self.0 {
                // SAFETY: same precondition
                super::RawOrBoxInner::Raw(s) => unsafe { s.drop_in_place(layout) },
                #[cfg(feature = "alloc")]
                // SAFETY: same precondition
                super::RawOrBoxInner::Box(s) => unsafe { s.drop_in_place(layout) },
            }
        }
    }

    // SAFETY: Both `Raw` and `Box` implements `StorageSend`
    unsafe impl<const SIZE: usize, const ALIGN: usize> StorageSend for super::RawOrBox<SIZE, ALIGN> where
        Align<ALIGN>: Alignment
    {
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
#[allow(clippy::undocumented_unsafe_blocks)]
mod tests {
    use core::{mem, mem::ManuallyDrop};

    use elain::{Align, Alignment};

    use crate::storage::{DropVTable, DynStorage, Storage, StorageMoved, StorageMut, VTable};

    impl VTable for DropVTable {
        fn drop_vtable(&self) -> &DropVTable {
            self
        }
    }
    type TestStorage<S> = DynStorage<S, DropVTable>;
    impl<S: Storage> TestStorage<S> {
        fn new_test<T>(data: T) -> Self {
            Self {
                storage: S::new(data),
                vtable: &const { DropVTable::new::<S, T>() },
            }
        }
    }

    #[test]
    fn raw_alignment() {
        fn check_alignment<const ALIGN: usize>()
        where
            Align<ALIGN>: Alignment,
        {
            let storages = [(); 2].map(TestStorage::<super::Raw<0, ALIGN>>::new_test);
            for s in &storages {
                assert!(s.ptr::<Align<ALIGN>>().is_aligned());
            }
            const { assert!(ALIGN < 2048) };
            assert!(
                storages
                    .iter()
                    .any(|s| !s.ptr::<Align<2048>>().is_aligned())
            );
        }
        check_alignment::<1>();
        check_alignment::<8>();
        check_alignment::<64>();
        check_alignment::<1024>();
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn raw_or_box() {
        fn check_variant<const N: usize>(variant: impl Fn(&super::RawOrBox<8>) -> bool) {
            let array = core::array::from_fn::<u8, N, _>(|i| i as u8);
            let storage = TestStorage::<super::RawOrBox<8>>::new_test(array);
            assert!(variant(&storage.storage));
            assert_eq!(unsafe { storage.ptr::<[u8; N]>().read() }, array);
        }
        check_variant::<4>(|s| matches!(s.0, super::RawOrBoxInner::Raw(_)));
        check_variant::<64>(|s| matches!(s.0, super::RawOrBoxInner::Box(_)));

        let storage = TestStorage::<super::RawOrBox<8, 1>>::new_test(0u64);
        assert!(matches!(storage.storage.0, super::RawOrBoxInner::Box(_)));
    }

    struct SetDropped<'a>(&'a mut bool);
    impl Drop for SetDropped<'_> {
        fn drop(&mut self) {
            assert!(!mem::replace(self.0, true));
        }
    }

    #[test]
    fn storage_drop() {
        fn check_drop<S: Storage>() {
            let mut dropped = false;
            let storage = TestStorage::<S>::new_test(SetDropped(&mut dropped));
            assert!(!*unsafe { storage.ptr::<SetDropped>().as_ref() }.0);
            drop(storage);
            assert!(dropped);
        }
        check_drop::<super::Raw<{ size_of::<SetDropped>() }, { align_of::<SetDropped>() }>>();
        #[cfg(feature = "alloc")]
        check_drop::<super::Box>();
        #[cfg(feature = "alloc")]
        check_drop::<super::Rc>();
        #[cfg(feature = "alloc")]
        check_drop::<super::Arc>();
        #[cfg(feature = "alloc")]
        check_drop::<super::RawOrBox<{ size_of::<SetDropped>() }>>();
        #[cfg(feature = "alloc")]
        check_drop::<super::RawOrBox<0>>();
    }

    #[test]
    fn storage_drop_moved() {
        fn check_drop_moved<S: StorageMut>() {
            let mut dropped = false;
            let mut storage =
                ManuallyDrop::new(TestStorage::<S>::new_test(SetDropped(&mut dropped)));
            let moved = unsafe {
                StorageMoved::<S, SetDropped>::new(DynStorage::move_storage(&mut storage))
            };
            unsafe { drop(moved.read()) };
            assert!(dropped);
        }
        check_drop_moved::<super::Raw<{ size_of::<SetDropped>() }, { align_of::<SetDropped>() }>>();
        #[cfg(feature = "alloc")]
        check_drop_moved::<super::Box>();
        #[cfg(feature = "alloc")]
        check_drop_moved::<super::RawOrBox<{ size_of::<SetDropped>() }>>();
        #[cfg(feature = "alloc")]
        check_drop_moved::<super::RawOrBox<0>>();
    }

    #[test]
    fn storage_dst() {
        fn check_dst<S: Storage>() {
            drop(TestStorage::<S>::new_test(()));
        }
        check_dst::<super::Raw<{ size_of::<SetDropped>() }, { align_of::<SetDropped>() }>>();
        #[cfg(feature = "alloc")]
        check_dst::<super::Box>();
        #[cfg(feature = "alloc")]
        check_dst::<super::Rc>();
        #[cfg(feature = "alloc")]
        check_dst::<super::Arc>();
        #[cfg(feature = "alloc")]
        check_dst::<super::RawOrBox<{ size_of::<SetDropped>() }>>();
        #[cfg(feature = "alloc")]
        check_dst::<super::RawOrBox<0>>();
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn clone() {
        fn check_clone<S: Storage + Clone>() {
            use core::sync::atomic::{AtomicBool, Ordering::Relaxed};
            // cannot use `&mut bool` because first `assert!(!dropped)` would invalid the tag
            struct SetDropped<'a>(&'a AtomicBool);
            impl Drop for SetDropped<'_> {
                fn drop(&mut self) {
                    assert!(!self.0.swap(true, Relaxed));
                }
            }
            let mut dropped = AtomicBool::new(false);
            let storage = TestStorage::<S>::new_test(SetDropped(&dropped));
            let storage2 = storage.clone();
            drop(storage);
            assert!(!dropped.load(Relaxed));
            drop(storage2);
            assert!(*dropped.get_mut());
        }
        check_clone::<super::Rc>();
        check_clone::<super::Arc>();
    }
}
