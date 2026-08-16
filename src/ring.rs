use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::Padded;

/// Bounded single-producer/single-consumer queue.
///
/// One producer must exclusively call `push`; one consumer must exclusively
/// call `pop`. Acquire/release publication makes initialized slots visible
/// without a mutex or a spin lock.
pub struct SpscRing<T: Copy, const N: usize> {
    slots: Box<[UnsafeCell<MaybeUninit<T>>; N]>,
    head: Padded<AtomicUsize>,
    tail: Padded<AtomicUsize>,
}

unsafe impl<T: Copy + Send, const N: usize> Send for SpscRing<T, N> {}
unsafe impl<T: Copy + Send, const N: usize> Sync for SpscRing<T, N> {}

impl<T: Copy, const N: usize> SpscRing<T, N> {
    pub fn new() -> Self {
        assert!(N.is_power_of_two(), "ring capacity must be a power of two");
        let slots = Box::new(core::array::from_fn(|_| {
            UnsafeCell::new(MaybeUninit::uninit())
        }));
        Self {
            slots,
            head: Padded(AtomicUsize::new(0)),
            tail: Padded(AtomicUsize::new(0)),
        }
    }

    #[inline(always)]
    pub fn push(&self, item: T) -> Result<(), T> {
        let head = self.head.0.load(Ordering::Relaxed);
        let next = head.wrapping_add(1);
        if next.wrapping_sub(self.tail.0.load(Ordering::Acquire)) > N {
            return Err(item);
        }
        unsafe { (*self.slots[head & (N - 1)].get()).write(item) };
        self.head.0.store(next, Ordering::Release);
        Ok(())
    }

    #[inline(always)]
    pub fn pop(&self) -> Option<T> {
        let tail = self.tail.0.load(Ordering::Relaxed);
        if tail == self.head.0.load(Ordering::Acquire) {
            return None;
        }
        let value = unsafe { (*self.slots[tail & (N - 1)].get()).assume_init_read() };
        self.tail.0.store(tail.wrapping_add(1), Ordering::Release);
        Some(value)
    }
}

impl<T: Copy, const N: usize> Default for SpscRing<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::SpscRing;

    #[test]
    fn fifo_and_full() {
        let ring = SpscRing::<u32, 4>::new();
        for i in 0..4 {
            assert_eq!(ring.push(i), Ok(()));
        }
        assert_eq!(ring.push(4), Err(4));
        for i in 0..4 {
            assert_eq!(ring.pop(), Some(i));
        }
        assert_eq!(ring.pop(), None);
    }
}
