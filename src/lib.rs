#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

pub mod ring;

pub const CACHE_LINE: usize = 64;
pub const BATCH_SIZE: usize = 64;
pub const MAX_DATAGRAM: usize = 2048;
pub const RING_CAPACITY: usize = 1 << 14;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PacketMeta {
    pub received_ns: u64,
    pub len: u16,
    pub checksum: u32,
}

#[repr(align(64))]
pub struct Padded<T>(pub T);

#[inline(always)]
pub fn packet_checksum(data: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    for &byte in data {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}
