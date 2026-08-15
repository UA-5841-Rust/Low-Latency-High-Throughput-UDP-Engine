use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

#[repr(align(64))]
struct CacheAligned<T>(T);
pub struct SpscRingBuffer<T, const N: usize> {
    head: CacheAligned<AtomicUsize>,
    
    // Align to 64 bytes to prevent False Sharing with head
    tail: CacheAligned<AtomicUsize>,

    // UnsafeCell allows interior mutability for buffer slots
    buffer: [UnsafeCell<Option<T>>; N],
}

unsafe impl<T: Send, const N: usize> Sync for SpscRingBuffer<T, N> {}

impl<T, const N: usize> SpscRingBuffer<T, N> {
    pub fn new() -> Self {
        let buffer = std::array::from_fn(|_| UnsafeCell::new(None));
        Self{
            head: CacheAligned(AtomicUsize::new(0)),
            tail: CacheAligned(AtomicUsize::new(0)),
            buffer,
        }
    }
    
    pub fn push(&self, value: T) -> Result<(), T> {
        let head = self.head.0.load(Ordering::Relaxed);
        // Acquire ensures we see the latest tail updates from Consumer
        let tail = self.tail.0.load(Ordering::Acquire);

        if head.wrapping_sub(tail) >= N {
            return Err(value);
        }

        unsafe {
            *self.buffer[head % N].get() = Some(value);
        }

        // Release ensures the memory write is visible before index updates
        self.head.0.store(head.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    pub fn pop(&self) -> Option<T> {
        let tail = self.tail.0.load(Ordering::Relaxed);
        // Acquire ensures we see the latest head updates from Producer
        let head = self.head.0.load(Ordering::Acquire); 

        if tail == head {
            return None; 
        }

        let value = unsafe {
            self.buffer[tail % N].get().replace(None).unwrap()
        };
        
        // Release ensures the memory read completes before index updates
        self.tail.0.store(tail.wrapping_add(1), Ordering::Release);
        Some(value)
    }
}