#[cfg(feature = "alloc")]
use alloc::{boxed::Box as StdBox, sync::Arc as StdArc};
use core::{
    marker::{PhantomData, PhantomPinned},
    mem::MaybeUninit,
    ptr::NonNull,
};

use elain::{Align, Alignment};

#[cfg(not(feature = "alloc"))]
pub type DefaultFnStorage = Raw<{ size_of::<usize>() }>;
#[cfg(feature = "alloc")]
pub type DefaultFnStorage = Box;
pub type DefaultFutureStorage = RawOrBox<{ 16 * size_of::<usize>() }>;

pub trait Storage: private::Storage {}
pub trait StorageMut: Storage + private::StorageMut {}

pub trait VTable: 'static {
    fn drop(&self) -> unsafe fn(NonNull<()>);
}

#[derive(Debug)]
pub(crate) struct DynStorage<S: Storage, VT: VTable> {
    pub(crate) storage: S,
    pub(crate) vtable: &'static VT,
}

impl<S: Storage, VT: VTable> DynStorage<S, VT> {
    pub(crate) fn ptr<T>(&self) -> NonNull<T> {
        self.storage.ptr().cast()
    }

    pub(crate) fn ptr_mut<T>(&mut self) -> NonNull<T> {
        self.storage.ptr_mut().cast()
    }
}

impl<S: Storage, VT: VTable> Drop for DynStorage<S, VT> {
    fn drop(&mut self) {
        unsafe { self.vtable.drop()(self.storage.ptr_mut()) }
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
    ptr: NonNull<T>,
    _phantom: PhantomData<S>,
}

impl<S: StorageMut, T> StorageMoved<S, T> {
    pub(crate) unsafe fn new(ptr: NonNull<()>) -> Self {
        Self {
            ptr: ptr.cast(),
            _phantom: PhantomData,
        }
    }

    pub(crate) unsafe fn read(&self) -> T {
        unsafe { self.ptr.read() }
    }
}

impl<S: StorageMut, T> Drop for StorageMoved<S, T> {
    fn drop(&mut self) {
        unsafe { S::drop_moved::<T>(self.ptr.cast()) }
    }
}

#[derive(Debug)]
#[repr(C)]
pub struct Raw<
    const SIZE: usize = { size_of::<usize>() },
    const ALIGN: usize = { align_of::<usize>() },
> where
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
    const unsafe fn new_unchecked<T>(data: T) -> Self {
        let mut raw = Self {
            data: MaybeUninit::uninit(),
            _align: Align::NEW,
            _not_send_sync: PhantomData,
            _pinned: PhantomPinned,
        };
        unsafe { raw.data.as_mut_ptr().cast::<T>().write(data) };
        raw
    }

    pub(crate) const fn new<T>(data: T) -> Self {
        const { assert!(size_of::<T>() <= SIZE) };
        const { assert!(align_of::<T>() <= ALIGN) };
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
        // The pointer has been obtained through `Arc::into_raw`,
        // and the `Arc` instance is still valid because strong
        // count is only decremented in drop.
        unsafe { StdArc::increment_strong_count(self.0.as_ptr()) }
        Self(self.0)
    }
}
#[cfg(feature = "alloc")]
impl Storage for Arc {}

#[cfg(not(feature = "alloc"))]
pub type RawOrBox<const SIZE: usize, const ALIGN: usize = { align_of::<usize>() }> =
    Raw<SIZE, ALIGN>;

#[cfg(feature = "alloc")]
#[derive(Debug)]
pub enum RawOrBox<const SIZE: usize, const ALIGN: usize = { align_of::<usize>() }>
where
    Align<ALIGN>: Alignment,
{
    Raw(Raw<SIZE, ALIGN>),
    Box(Box),
}

#[cfg(feature = "alloc")]
impl<const SIZE: usize, const ALIGN: usize> Storage for RawOrBox<SIZE, ALIGN> where
    Align<ALIGN>: Alignment
{
}
#[cfg(feature = "alloc")]
impl<const SIZE: usize, const ALIGN: usize> StorageMut for RawOrBox<SIZE, ALIGN> where
    Align<ALIGN>: Alignment
{
}

pub(crate) mod private {
    #[cfg(feature = "alloc")]
    use alloc::{boxed::Box, sync::Arc};
    use core::{mem::MaybeUninit, ptr::NonNull};

    use elain::{Align, Alignment};

    pub trait Storage: Sized + 'static {
        fn new<T>(data: T) -> Self;
        fn ptr(&self) -> NonNull<()>;
        fn ptr_mut(&mut self) -> NonNull<()>;
        unsafe fn drop<T>(ptr_mut: NonNull<()>);
    }

    pub trait StorageMut: Storage {
        unsafe fn drop_moved<T>(ptr_mut: NonNull<MaybeUninit<T>>);
    }

    impl<const SIZE: usize, const ALIGN: usize> Storage for super::Raw<SIZE, ALIGN>
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
        unsafe fn drop<T>(ptr_mut: NonNull<()>) {
            unsafe { ptr_mut.cast::<T>().drop_in_place() }
        }
    }

    impl<const SIZE: usize, const ALIGN: usize> StorageMut for super::Raw<SIZE, ALIGN>
    where
        Align<ALIGN>: Alignment,
    {
        unsafe fn drop_moved<T>(_ptr_mut: NonNull<MaybeUninit<T>>) {}
    }

    #[cfg(feature = "alloc")]
    impl Storage for super::Box {
        fn new<T>(data: T) -> Self {
            Self::new_box(Box::new(data))
        }
        fn ptr(&self) -> NonNull<()> {
            self.0
        }
        fn ptr_mut(&mut self) -> NonNull<()> {
            self.0
        }
        unsafe fn drop<T>(ptr_mut: NonNull<()>) {
            drop(unsafe { Box::<T>::from_raw(ptr_mut.cast().as_ptr()) })
        }
    }
    #[cfg(feature = "alloc")]
    impl StorageMut for super::Box {
        unsafe fn drop_moved<T>(ptr_mut: NonNull<MaybeUninit<T>>) {
            drop(unsafe { Box::from_raw(ptr_mut.as_ptr()) });
        }
    }

    #[cfg(feature = "alloc")]
    impl Storage for super::Arc {
        fn new<T>(data: T) -> Self {
            Self::new_arc(Arc::new(data))
        }
        fn ptr(&self) -> NonNull<()> {
            self.0
        }
        fn ptr_mut(&mut self) -> NonNull<()> {
            self.0
        }
        unsafe fn drop<T>(ptr_mut: NonNull<()>) {
            drop(unsafe { Arc::<T>::from_raw(ptr_mut.cast().as_ptr()) });
        }
    }

    #[cfg(feature = "alloc")]
    impl<const SIZE: usize, const ALIGN: usize> Storage for super::RawOrBox<SIZE, ALIGN>
    where
        Align<ALIGN>: Alignment,
    {
        // It prevents 100% coverage, maybe because of
        // https://github.com/taiki-e/cargo-llvm-cov/issues/394
        #[cfg_attr(coverage_nightly, coverage(off))]
        fn new<T>(data: T) -> Self {
            if size_of::<T>() <= SIZE && align_of::<T>() <= ALIGN {
                Self::Raw(unsafe { super::Raw::new_unchecked(data) })
            } else {
                Self::Box(super::Box::new(data))
            }
        }
        fn ptr(&self) -> NonNull<()> {
            match self {
                Self::Raw(s) => s.ptr(),
                Self::Box(s) => s.ptr(),
            }
        }
        fn ptr_mut(&mut self) -> NonNull<()> {
            match self {
                Self::Raw(s) => s.ptr_mut(),
                Self::Box(s) => s.ptr_mut(),
            }
        }
        #[cfg_attr(coverage_nightly, coverage(off))] // See new
        unsafe fn drop<T>(ptr_mut: NonNull<()>) {
            if size_of::<T>() <= SIZE && align_of::<T>() <= ALIGN {
                // SAFETY: same precondition
                unsafe { super::Raw::<SIZE, ALIGN>::drop::<T>(ptr_mut) };
            } else {
                // SAFETY: same precondition
                unsafe { super::Box::drop::<T>(ptr_mut) };
            }
        }
    }
    #[cfg(feature = "alloc")]
    impl<const SIZE: usize, const ALIGN: usize> StorageMut for super::RawOrBox<SIZE, ALIGN>
    where
        Align<ALIGN>: Alignment,
    {
        #[cfg_attr(coverage_nightly, coverage(off))] // See new
        unsafe fn drop_moved<T>(ptr_mut: NonNull<MaybeUninit<T>>) {
            if size_of::<T>() <= SIZE && align_of::<T>() <= ALIGN {
                // SAFETY: same precondition
                unsafe { super::Raw::<SIZE, ALIGN>::drop_moved::<T>(ptr_mut) };
            } else {
                // SAFETY: same precondition
                unsafe { super::Box::drop_moved::<T>(ptr_mut) };
            }
        }
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod tests {
    use core::{mem, mem::ManuallyDrop, ptr::NonNull};

    use elain::{Align, Alignment};

    use crate::storage::{DynStorage, Storage, StorageMoved, StorageMut, VTable};

    struct TestVTable {
        drop: unsafe fn(NonNull<()>),
    }
    impl VTable for TestVTable {
        fn drop(&self) -> unsafe fn(NonNull<()>) {
            self.drop
        }
    }
    type TestStorage<S> = DynStorage<S, TestVTable>;
    impl<S: Storage> TestStorage<S> {
        fn new_test<T>(data: T) -> Self {
            Self {
                storage: S::new(data),
                vtable: &TestVTable { drop: S::drop::<T> },
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
            assert_eq!(unsafe { storage.ptr::<[u8; N]>().read() }, array)
        }
        check_variant::<4>(|s| matches!(s, super::RawOrBox::Raw(_)));
        check_variant::<64>(|s| matches!(s, super::RawOrBox::Box(_)));

        let storage = TestStorage::<super::RawOrBox<8, 1>>::new_test(0u64);
        assert!(matches!(storage.storage, super::RawOrBox::Box(_)));
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
        check_drop::<super::RawOrBox<{ size_of::<SetDropped>() }>>();
        #[cfg(feature = "alloc")]
        check_drop::<super::RawOrBox<0>>();
        #[cfg(feature = "alloc")]
        check_drop::<super::Arc>();
    }

    #[test]
    fn storage_drop_moved() {
        fn check_drop_moved<S: StorageMut>() {
            let mut dropped = false;
            let mut storage =
                ManuallyDrop::new(TestStorage::<S>::new_test(SetDropped(&mut dropped)));
            let moved = unsafe { StorageMoved::<S, SetDropped>::new(storage.ptr_mut()) };
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

    #[cfg(feature = "alloc")]
    #[test]
    fn arc_clone() {
        use core::sync::atomic::{AtomicBool, Ordering::Relaxed};
        // cannot use `&mut bool` because first `assert!(!dropped)` would invalid the tag
        struct SetDropped<'a>(&'a AtomicBool);
        impl Drop for SetDropped<'_> {
            fn drop(&mut self) {
                assert!(!self.0.swap(true, Relaxed));
            }
        }
        let mut dropped = AtomicBool::new(false);
        let storage = TestStorage::<super::Arc>::new_test(SetDropped(&dropped));
        let storage2 = storage.clone();
        drop(storage);
        assert!(!dropped.load(Relaxed));
        drop(storage2);
        assert!(*dropped.get_mut());
    }
}
