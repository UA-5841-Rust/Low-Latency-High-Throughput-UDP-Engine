use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[repr(align(64))]
#[derive(Default)]
struct CachePadded<T> {
    value: T,
}

struct Shared<T> {
    buf: Box<[UnsafeCell<MaybeUninit<T>>]>,
    mask: usize,                    // capacity - 1; capacity is a power of two
    head: CachePadded<AtomicUsize>, // next slot the producer will write
    tail: CachePadded<AtomicUsize>, // next slot the consumer will read
}

unsafe impl<T: Send> Sync for Shared<T> {}

impl<T> Drop for Shared<T> {
    fn drop(&mut self) {
        let mut tail = *self.tail.value.get_mut();
        let head = *self.head.value.get_mut();
        while tail != head {
            unsafe {
                (*self.buf[tail].get()).assume_init_drop();
            }
            tail = (tail + 1) & self.mask;
        }
    }
}

pub struct Producer<T> {
    shared: Arc<Shared<T>>,
}

pub struct Consumer<T> {
    shared: Arc<Shared<T>>,
}

impl<T> Producer<T> {
    /// Tries to push `value`. On failure (ring full -- the consumer hasn't
    /// kept up) hands `value` back so the caller can decide what to do
    /// (drop it, retry, count it as a loss). Never blocks, never allocates.
    #[inline]
    pub fn push(&mut self, value: T) -> Result<(), T> {
        let head = self.shared.head.value.load(Ordering::Relaxed);
        let next = (head + 1) & self.shared.mask;

        // If this sees the consumer's latest `tail`, it also sees
        // every slot read the consumer completed before publishing it.
        if next == self.shared.tail.value.load(Ordering::Acquire) {
            return Err(value); // full
        }

        unsafe {
            (*self.shared.buf[head].get()).write(value);
        }

        self.shared.head.value.store(next, Ordering::Release);
        Ok(())
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        let head = self.shared.head.value.load(Ordering::Relaxed);
        let next = (head + 1) & self.shared.mask;
        next == self.shared.tail.value.load(Ordering::Acquire)
    }
}

impl<T> Consumer<T> {
    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        let tail = self.shared.tail.value.load(Ordering::Relaxed);

        // If this sees the producer's latest `head`, it also sees
        // the write into this slot that happened before that publish.
        if tail == self.shared.head.value.load(Ordering::Acquire) {
            return None; // empty
        }

        let value = unsafe { (*self.shared.buf[tail].get()).assume_init_read() };

        self.shared
            .tail
            .value
            .store((tail + 1) & self.shared.mask, Ordering::Release);
        Some(value)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        let tail = self.shared.tail.value.load(Ordering::Relaxed);
        tail == self.shared.head.value.load(Ordering::Acquire)
    }
}

/// Creates a bounded SPSC ring buffer with `capacity - 1` usable slots (one
/// slot always stays empty so "full" and "empty" can be told apart without
/// a separate counter -- the classic trick for this kind of ring buffer).
///
/// `capacity` must be a power of two so the hot-path index wrap can use
/// `& (capacity - 1)` instead of a division (`%`).
///
pub fn spsc<T>(capacity: usize) -> (Producer<T>, Consumer<T>) {
    assert!(
        capacity.is_power_of_two() && capacity > 1,
        "ring buffer capacity must be a power of two > 1, got {capacity}"
    );

    let mut v = Vec::with_capacity(capacity);
    for _ in 0..capacity {
        v.push(UnsafeCell::new(MaybeUninit::uninit()));
    }
    let buf = v.into_boxed_slice();

    let shared = Arc::new(Shared {
        buf,
        mask: capacity - 1,
        head: CachePadded::default(),
        tail: CachePadded::default(),
    });

    (
        Producer {
            shared: shared.clone(),
        },
        Consumer { shared },
    )
}
