#[cfg(feature = "alloc")]
use alloc::{boxed::Box as StdBox, sync::Arc as StdArc};
use core::{
    fmt,
    marker::{PhantomData, PhantomPinned},
    mem,
    mem::MaybeUninit,
    ptr::NonNull,
};

use elain::{Align, Alignment};

#[cfg(not(feature = "alloc"))]
pub type DefaultFnStorage = Raw<{ size_of::<usize>() }>;
#[cfg(feature = "alloc")]
pub type DefaultFnStorage = Box;
pub type DefaultFutureStorage = RawOrBox<{ 16 * size_of::<usize>() }>;

pub trait Storage: private::Storage + fmt::Debug {}
pub trait StorageMut: Storage {}

#[derive(Debug, Clone)]
pub(crate) struct StorageImpl<S: Storage> {
    inner: S,
    drop: unsafe fn(NonNull<()>),
}

impl<S: Storage> StorageImpl<S> {
    const fn new_impl<T>(inner: S) -> Self {
        let drop = S::drop::<T>;
        Self { inner, drop }
    }

    pub(crate) fn new<T>(data: T) -> Self {
        Self::new_impl::<T>(S::new(data))
    }

    pub(crate) fn ptr<T>(&self) -> NonNull<T> {
        self.inner.ptr().cast()
    }

    pub(crate) fn ptr_mut<T>(&mut self) -> NonNull<T> {
        self.inner.ptr_mut().cast()
    }
}

impl<S: Storage> Drop for StorageImpl<S> {
    fn drop(&mut self) {
        unsafe { (self.drop)(self.inner.ptr_mut()) }
    }
}

impl<const SIZE: usize, const ALIGN: usize> StorageImpl<Raw<SIZE, ALIGN>>
where
    Align<ALIGN>: Alignment,
{
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) const fn new_raw<T>(data: T) -> Self {
        Self::new_impl::<T>(Raw::new(data))
    }
}

#[cfg(feature = "alloc")]
impl StorageImpl<Box> {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn new_box<T>(data: StdBox<T>) -> Self {
        Self::new_impl::<T>(Box::new_box(data))
    }
}

#[cfg(feature = "alloc")]
impl StorageImpl<Arc> {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn new_arc<T>(data: StdArc<T>) -> Self {
        Self::new_impl::<T>(Arc::new_arc(data))
    }
}

#[cfg(feature = "alloc")]
impl<const SIZE: usize, const ALIGN: usize> StorageImpl<RawOrBox<SIZE, ALIGN>>
where
    Align<ALIGN>: Alignment,
{
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) const fn new_raw2<T>(data: T) -> Self {
        Self::new_impl::<T>(RawOrBox::Raw(Raw::new(data)))
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn new_box2<T>(data: StdBox<T>) -> Self {
        Self::new_impl::<T>(RawOrBox::Box(Box::new_box(data)))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct StorageOnceImpl<S: StorageMut> {
    inner: S,
    drop: unsafe fn(NonNull<()>, bool),
}

impl<S: StorageMut> StorageOnceImpl<S> {
    const fn new_impl<T>(inner: S) -> Self {
        let drop = S::drop_once::<T>;
        Self { inner, drop }
    }

    pub(crate) fn new<T>(data: T) -> Self {
        Self::new_impl::<T>(S::new(data))
    }

    pub(crate) fn ptr_once<T>(&mut self) -> NonNull<T> {
        self.inner.ptr_mut().cast()
    }

    pub(crate) fn drop(mut self, moved: bool) {
        unsafe { (self.drop)(self.inner.ptr_mut(), moved) }
        mem::forget(self);
    }
}

impl<S: StorageMut> Drop for StorageOnceImpl<S> {
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn drop(&mut self) {
        unreachable!()
    }
}

impl<const SIZE: usize, const ALIGN: usize> StorageOnceImpl<Raw<SIZE, ALIGN>>
where
    Align<ALIGN>: Alignment,
{
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) const fn new_raw<T>(data: T) -> Self {
        Self::new_impl::<T>(Raw::new(data))
    }
}

#[cfg(feature = "alloc")]
impl StorageOnceImpl<Box> {
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn new_box<T>(data: StdBox<T>) -> Self {
        Self::new_impl::<T>(Box::new_box(data))
    }
}

#[cfg(feature = "alloc")]
impl<const SIZE: usize, const ALIGN: usize> StorageOnceImpl<RawOrBox<SIZE, ALIGN>>
where
    Align<ALIGN>: Alignment,
{
    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) const fn new_raw2<T>(data: T) -> Self {
        Self::new_impl::<T>(RawOrBox::Raw(Raw::new(data)))
    }

    #[cfg_attr(coverage_nightly, coverage(off))]
    pub(crate) fn new_box2<T>(data: StdBox<T>) -> Self {
        Self::new_impl::<T>(RawOrBox::Box(Box::new_box(data)))
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

    const fn new<T>(data: T) -> Self {
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
    fn new_box<T>(data: StdBox<T>) -> Self {
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
    fn new_arc<T>(data: StdArc<T>) -> Self {
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

mod private {
    #[cfg(feature = "alloc")]
    use alloc::{boxed::Box, sync::Arc};
    use core::ptr::NonNull;

    use elain::{Align, Alignment};

    pub trait Storage: Sized {
        fn new<T>(data: T) -> Self;
        fn ptr(&self) -> NonNull<()>;
        fn ptr_mut(&mut self) -> NonNull<()>;
        unsafe fn drop<T>(ptr_mut: NonNull<()>);
        unsafe fn drop_once<T>(ptr_mut: NonNull<()>, moved: bool);
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
        unsafe fn drop_once<T>(ptr_mut: NonNull<()>, moved: bool) {
            if !moved {
                unsafe { ptr_mut.cast::<T>().drop_in_place() };
            }
        }
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
        unsafe fn drop_once<T>(ptr_mut: NonNull<()>, moved: bool) {
            if !moved {
                unsafe { ptr_mut.cast::<T>().drop_in_place() }
            }
            drop(unsafe { Box::<core::mem::ManuallyDrop<T>>::from_raw(ptr_mut.cast().as_ptr()) });
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
        #[cfg_attr(coverage_nightly, coverage(off))]
        unsafe fn drop_once<T>(_ptr_mut: NonNull<()>, _moved: bool) {
            unreachable!()
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
        #[cfg_attr(coverage_nightly, coverage(off))] // See new
        unsafe fn drop_once<T>(ptr_mut: NonNull<()>, moved: bool) {
            if size_of::<T>() <= SIZE && align_of::<T>() <= ALIGN {
                // SAFETY: same precondition
                unsafe { super::Raw::<SIZE, ALIGN>::drop_once::<T>(ptr_mut, moved) };
            } else {
                // SAFETY: same precondition
                unsafe { super::Box::drop_once::<T>(ptr_mut, moved) };
            }
        }
    }
}

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod tests {
    use core::mem;

    use elain::{Align, Alignment};

    use crate::storage::{Storage, StorageImpl, StorageMut, StorageOnceImpl};

    #[test]
    fn raw_alignment() {
        fn check_alignment<const ALIGN: usize>()
        where
            Align<ALIGN>: Alignment,
        {
            let storages = [(); 2].map(StorageImpl::<super::Raw<0, ALIGN>>::new);
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
            let storage = StorageImpl::<super::RawOrBox<8>>::new(array);
            assert!(variant(&storage.inner));
            assert_eq!(unsafe { storage.ptr::<[u8; N]>().read() }, array)
        }
        check_variant::<4>(|s| matches!(s, super::RawOrBox::Raw(_)));
        check_variant::<64>(|s| matches!(s, super::RawOrBox::Box(_)));

        let storage = StorageImpl::<super::RawOrBox<8, 1>>::new(0u64);
        assert!(matches!(storage.inner, super::RawOrBox::Box(_)));
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
            let storage = StorageImpl::<S>::new(SetDropped(&mut dropped));
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
    fn storage_once_drop() {
        fn check_drop<S: StorageMut>() {
            for moved in [true, false] {
                let mut dropped = false;
                let mut storage = StorageOnceImpl::<S>::new(SetDropped(&mut dropped));
                if moved {
                    unsafe { storage.ptr_once::<SetDropped>().read() };
                    assert!(dropped);
                }
                storage.drop(moved);
                assert!(dropped);
            }
        }
        check_drop::<super::Raw<{ size_of::<SetDropped>() }, { align_of::<SetDropped>() }>>();
        #[cfg(feature = "alloc")]
        check_drop::<super::Box>();
        #[cfg(feature = "alloc")]
        check_drop::<super::RawOrBox<{ size_of::<SetDropped>() }>>();
        #[cfg(feature = "alloc")]
        check_drop::<super::RawOrBox<0>>();
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
        let storage = StorageImpl::<super::Arc>::new(SetDropped(&dropped));
        let storage2 = storage.clone();
        drop(storage);
        assert!(!dropped.load(Relaxed));
        drop(storage2);
        assert!(*dropped.get_mut());
    }
}
