use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use crate::server::Packet;

pub const RING_SIZE: usize = 8192;

#[repr(align(64))]
pub struct SpscQueue {
    head: AtomicUsize,
    _pad1: [u8; 56],
    tail: AtomicUsize,
    _pad2: [u8; 56],
    buffer: UnsafeCell<[Packet; RING_SIZE]>,
}

unsafe impl Sync for SpscQueue {}

impl SpscQueue {
    pub fn new() -> Box<Self> {
        unsafe {
            let layout = std::alloc::Layout::new::<Self>();
            let ptr = std::alloc::alloc_zeroed(layout) as *mut Self;
            if ptr.is_null() {
                std::alloc::handle_alloc_error(layout);
            }
            Box::from_raw(ptr)
        }
    }

    pub fn push(&self, packet: &Packet) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let next_head = (head + 1) % RING_SIZE;

        if next_head == self.tail.load(Ordering::Acquire) {
            return false;
        }

        unsafe {
            let buf = self.buffer.get();
            (*buf)[head] = *packet;
        }

        self.head.store(next_head, Ordering::Release);
        true
    }

    pub fn pop(&self) -> Option<Packet> {
        let tail = self.tail.load(Ordering::Relaxed);

        if tail == self.head.load(Ordering::Acquire) {
            return None;
        }

        let packet = unsafe {
            let buf = self.buffer.get();
            (*buf)[tail]
        };

        self.tail.store((tail + 1) % RING_SIZE, Ordering::Release);
        Some(packet)
    }
}