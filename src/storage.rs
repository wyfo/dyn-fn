#[cfg(feature = "alloc")]
use alloc::{boxed::Box as StdBox, sync::Arc as StdArc};
use core::{
    fmt,
    marker::{PhantomData, PhantomPinned},
    mem,
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use elain::{Align, Alignment};

#[cfg(not(feature = "alloc"))]
pub type DefaultFnStorage = Raw<{ size_of::<usize>() }>;
#[cfg(feature = "alloc")]
pub type DefaultFnStorage = Box;
#[cfg(not(feature = "alloc"))]
pub type DefaultFutureStorage = Raw<{ 16 * size_of::<usize>() }>;
#[cfg(feature = "alloc")]
pub type DefaultFutureStorage = RawOrBox<{ 16 * size_of::<usize>() }>;

pub trait Storage: private::Storage + fmt::Debug {}
pub trait StorageMut: Storage {}

type NewStorage<T> = (T, unsafe fn(NonNull<()>), unsafe fn(NonNull<()>, bool));

#[derive(Debug, Clone)]
pub(crate) struct StorageImpl<S: Storage> {
    inner: S,
    drop: unsafe fn(NonNull<()>),
}

impl<S: Storage> StorageImpl<S> {
    pub(crate) fn new<T>(data: T) -> Self {
        let (inner, drop, _) = S::new(data);
        Self { inner, drop }
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

#[derive(Debug, Clone)]
pub(crate) struct StorageOnceImpl<S: StorageMut> {
    inner: S,
    drop: unsafe fn(NonNull<()>, bool),
}

impl<S: StorageMut> StorageOnceImpl<S> {
    pub(crate) fn new<T>(data: T) -> Self {
        let (inner, _, drop) = S::new(data);
        Self { inner, drop }
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
    unsafe fn new_unchecked<T>(data: T) -> NewStorage<Self> {
        let mut raw = Self {
            data: MaybeUninit::uninit(),
            _align: Align::NEW,
            _not_send_sync: PhantomData,
            _pinned: PhantomPinned,
        };
        unsafe { raw.data.as_mut_ptr().cast::<T>().write(data) };
        (
            raw,
            |data| unsafe { data.cast::<T>().drop_in_place() },
            |data, moved| match moved {
                true => {}
                false => unsafe { data.cast::<T>().drop_in_place() },
            },
        )
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
    pub(crate) fn new_box<T>(data: StdBox<T>) -> NewStorage<Self> {
        (
            Self(NonNull::new(StdBox::into_raw(data).cast()).unwrap()),
            |data| drop(unsafe { StdBox::<T>::from_raw(data.cast().as_ptr()) }),
            |data, moved| match moved {
                true => {
                    drop(unsafe { StdBox::<mem::ManuallyDrop<T>>::from_raw(data.cast().as_ptr()) })
                }
                false => drop(unsafe { StdBox::<T>::from_raw(data.cast().as_ptr()) }),
            },
        )
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
    pub(crate) fn new_arc<T>(data: StdArc<T>) -> NewStorage<Self> {
        #[cfg_attr(coverage_nightly, coverage(off))]
        fn drop_once(_: NonNull<()>, _: bool) {
            unreachable!()
        }
        (
            Self(NonNull::new(StdArc::into_raw(data).cast_mut().cast()).unwrap()),
            |data| drop(unsafe { StdArc::<T>::from_raw(data.cast().as_ptr()) }),
            drop_once,
        )
    }
}
#[cfg(feature = "alloc")]
impl Clone for Arc {
    fn clone(&self) -> Self {
        unsafe { StdArc::increment_strong_count(self.0.as_ptr()) }
        Self(self.0)
    }
}
#[cfg(feature = "alloc")]
impl Storage for Arc {}

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
    use core::ptr::NonNull;

    use elain::{Align, Alignment};

    use crate::storage::NewStorage;

    pub trait Storage: Sized {
        fn new<T>(data: T) -> NewStorage<Self>;
        fn ptr(&self) -> NonNull<()>;
        fn ptr_mut(&mut self) -> NonNull<()>;
    }

    impl<const SIZE: usize, const ALIGN: usize> Storage for super::Raw<SIZE, ALIGN>
    where
        Align<ALIGN>: Alignment,
    {
        fn new<T>(data: T) -> (Self, unsafe fn(NonNull<()>), unsafe fn(NonNull<()>, bool)) {
            const { assert!(size_of::<T>() <= SIZE) };
            const { assert!(align_of::<T>() <= ALIGN) };
            unsafe { Self::new_unchecked::<T>(data) }
        }
        fn ptr(&self) -> NonNull<()> {
            NonNull::from(&self.data).cast()
        }
        fn ptr_mut(&mut self) -> NonNull<()> {
            NonNull::from(&mut self.data).cast()
        }
    }

    #[cfg(feature = "alloc")]
    impl Storage for super::Box {
        fn new<T>(data: T) -> (Self, unsafe fn(NonNull<()>), unsafe fn(NonNull<()>, bool)) {
            Self::new_box(alloc::boxed::Box::new(data))
        }
        fn ptr(&self) -> NonNull<()> {
            self.0
        }
        fn ptr_mut(&mut self) -> NonNull<()> {
            self.0
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
        fn new<T>(data: T) -> (Self, unsafe fn(NonNull<()>), unsafe fn(NonNull<()>, bool)) {
            if size_of::<T>() <= SIZE && align_of::<T>() <= ALIGN {
                let (storage, drop, drop_once) = unsafe { super::Raw::new_unchecked(data) };
                (Self::Raw(storage), drop, drop_once)
            } else {
                let (storage, drop, drop_once) = super::Box::new_box(alloc::boxed::Box::new(data));
                (Self::Box(storage), drop, drop_once)
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
    }

    #[cfg(feature = "alloc")]
    impl Storage for super::Arc {
        fn new<T>(data: T) -> (Self, unsafe fn(NonNull<()>), unsafe fn(NonNull<()>, bool)) {
            Self::new_arc(alloc::sync::Arc::new(data))
        }
        fn ptr(&self) -> NonNull<()> {
            self.0
        }
        fn ptr_mut(&mut self) -> NonNull<()> {
            self.0
        }
    }
}

pub(crate) struct SendWrapper<S>(S);
impl<S> SendWrapper<S> {
    pub(crate) unsafe fn new(storage: S) -> Self {
        Self(storage)
    }
}
unsafe impl<S> Send for SendWrapper<S> {}
impl<S> Deref for SendWrapper<S> {
    type Target = S;
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<S> DerefMut for SendWrapper<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
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
        check_drop::<super::RawOrBox<{ size_of::<SetDropped>() }, { align_of::<SetDropped>() }>>();
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
        check_drop::<super::RawOrBox<{ size_of::<SetDropped>() }, { align_of::<SetDropped>() }>>();
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
