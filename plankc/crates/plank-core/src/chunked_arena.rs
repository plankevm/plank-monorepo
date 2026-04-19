use allocator_api2::alloc::{AllocError, Allocator, Global, handle_alloc_error};
use std::{alloc::Layout, cell::Cell, ptr::NonNull};

const MAX_CHUNKS: u32 = 22;
const FIRST_CHUNK_SIZE_BYTES: usize = 1024;

const _MAX_BYTES_FITS_IN_U32: () =
    assert!(FIRST_CHUNK_SIZE_BYTES as u64 * 2u64.pow(MAX_CHUNKS) == 2u64.pow(u32::BITS));

fn chunk_index_to_size(chunk_index: u32) -> usize {
    let size_exponent = chunk_index.saturating_sub(1);
    FIRST_CHUNK_SIZE_BYTES << size_exponent
}

fn chunk_index_to_start_offset(chunk_index: u32) -> usize {
    if chunk_index == 0 {
        return 0;
    }
    FIRST_CHUNK_SIZE_BYTES << (chunk_index - 1)
}

fn offset_to_chunk(offset: usize) -> (usize, usize) {
    if offset < FIRST_CHUNK_SIZE_BYTES {
        return (0, offset);
    }
    let first_chunk_size_multiples = offset / FIRST_CHUNK_SIZE_BYTES;
    let chunk_index = first_chunk_size_multiples.ilog2() + 1;
    let size_exponent = first_chunk_size_multiples.ilog2();
    let chunk_start_offset = FIRST_CHUNK_SIZE_BYTES << size_exponent;
    (chunk_index as usize, offset - chunk_start_offset)
}

fn chunk_layout(chunk_index: u32, align: usize) -> Layout {
    unsafe {
        let size = chunk_index_to_size(chunk_index);
        Layout::from_size_align_unchecked(size, align)
    }
}

/// Unlike a normal `bumpalo`-style arena [`ChunkedArena`] gives you both a stable pointer when you
/// allocate as well as a stable `u32` offset you can store and then use to retrieve the associated
/// pointer later.
pub struct ChunkedArena<const ALIGN: usize, A: Allocator = Global> {
    chunk_index: Cell<u32>,
    chunk_rel_offset: Cell<u32>,
    chunk_bytes_remaining: Cell<u32>,
    chunks: [Cell<Option<NonNull<u8>>>; MAX_CHUNKS as usize],
    alloc: A,
}

impl<const ALIGN: usize> Default for ChunkedArena<ALIGN> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const ALIGN: usize> ChunkedArena<ALIGN> {
    pub const fn new() -> Self {
        Self::new_in(Global)
    }
}

impl<const ALIGN: usize, A: Allocator> ChunkedArena<ALIGN, A> {
    pub const fn new_in(alloc: A) -> Self {
        const { assert!(ALIGN > 0 && ALIGN.is_power_of_two(), "invalid alignment") };
        const { assert!(FIRST_CHUNK_SIZE_BYTES.is_multiple_of(ALIGN), "alignment too large") };
        Self {
            chunk_index: Cell::new(0),
            chunk_rel_offset: Cell::new(0),
            chunk_bytes_remaining: Cell::new(FIRST_CHUNK_SIZE_BYTES as u32),
            chunks: [const { Cell::new(None) }; MAX_CHUNKS as usize],
            alloc,
        }
    }

    /// Allocate `size` bytes of append-only storage.
    ///
    /// Returns the stable offset and a write pointer into the arena.
    /// The returned pointer is `ALIGN`-aligned and the offset is a multiple of `ALIGN`;
    /// both remain valid for the lifetime of the arena regardless of subsequent allocations.
    ///
    /// # Safety
    ///
    /// The caller must not assume that the returned pointer points to initialized bytes.
    /// Furthermore the caller may only assume that the pointer returned by [`get`](Self::get)
    /// using the given offset points to initialized data if they've initialized it.
    pub unsafe fn alloc_append(&self, min_size: usize) -> (u32, *mut u8) {
        let size = min_size.next_multiple_of(ALIGN);

        let mut chunk_index = self.chunk_index.get();
        let mut chunk_rel_offset = self.chunk_rel_offset.get();
        let mut chunk_bytes_remaining = self.chunk_bytes_remaining.get() as usize;

        while size > chunk_bytes_remaining {
            chunk_index += 1;
            assert!(chunk_index < MAX_CHUNKS, "out of chunks");
            self.chunk_index.set(chunk_index);
            chunk_rel_offset = 0;
            chunk_bytes_remaining = chunk_index_to_size(chunk_index);
        }

        let chunk_base = if chunk_rel_offset == 0 {
            if size == 0 {
                NonNull::dangling()
            } else {
                unsafe { self.allocate_chunk(chunk_index) }
            }
        } else {
            // Safety: This branch is only reached if `chunk_rel_offset` is non-zero which is only
            // possible if the same chunk was already allocated.
            unsafe { self.chunks[chunk_index as usize].get().unwrap_unchecked() }
        };

        let write_ptr = unsafe { chunk_base.as_ptr().byte_add(chunk_rel_offset as usize) };
        self.chunk_rel_offset.set(chunk_rel_offset + size as u32);
        self.chunk_bytes_remaining.set(chunk_bytes_remaining as u32 - size as u32);
        let chunk_start_offset = chunk_index_to_start_offset(chunk_index);
        (chunk_start_offset as u32 + chunk_rel_offset, write_ptr)
    }

    /// # Safety
    /// Expects a valid `chunk_index` less than [`MAX_CHUNKS`] and has not been allocated yet.
    unsafe fn allocate_chunk(&self, chunk_index: u32) -> NonNull<u8> {
        unsafe {
            let layout = chunk_layout(chunk_index, ALIGN);
            let new_ptr = self
                .alloc
                .allocate(layout)
                .unwrap_or_else(|AllocError| handle_alloc_error(layout))
                .cast();
            let prev_ptr = self.chunks[chunk_index as usize].replace(Some(new_ptr));
            #[cfg(debug_assertions)]
            if let Some(prev_ptr) = prev_ptr {
                self.alloc.deallocate(prev_ptr, layout);
                unreachable!("invariant: chunk reallocated");
            }
            new_ptr
        }
    }

    /// Resolve a previously returned offset to a stable pointer.
    ///
    /// # Safety
    /// Requires `offset` to be derived from `alloc_append` called on the same struct.
    /// Furthermore data pointed to by the returned pointer which is part of the original
    /// allocation *MUST NOT* by mutated.
    pub unsafe fn get(&self, offset: u32) -> *const u8 {
        unsafe {
            let (chunk_index, rel_offset) = offset_to_chunk(offset as usize);
            match self.chunks[chunk_index].get() {
                Some(chunk_base_ptr) => chunk_base_ptr.as_ptr().byte_add(rel_offset),
                None => core::ptr::null(),
            }
        }
    }
}

impl<const ALIGN: usize, A: Allocator> Drop for ChunkedArena<ALIGN, A> {
    fn drop(&mut self) {
        let last_chunk_index = self.chunk_index.get();
        for chunk_index in 0..=last_chunk_index {
            if let Some(ptr) = self.chunks[chunk_index as usize].get() {
                // Safety: We only set a chunk's pointer to be `Some` upon successful allocation
                unsafe {
                    let layout = chunk_layout(chunk_index, ALIGN);
                    self.alloc.deallocate(ptr, layout)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offset_to_chunk() {
        assert_eq!(offset_to_chunk(0), (0, 0));
        assert_eq!(offset_to_chunk(34), (0, 34));
        assert_eq!(offset_to_chunk(1023), (0, 1023));

        assert_eq!(offset_to_chunk(1024), (1, 0));
        assert_eq!(offset_to_chunk(1060), (1, 36));
        assert_eq!(offset_to_chunk(2047), (1, 1023));

        assert_eq!(offset_to_chunk(2048), (2, 0));
        assert_eq!(offset_to_chunk(3000), (2, 952));
        assert_eq!(offset_to_chunk(3072), (2, 1024));
        assert_eq!(offset_to_chunk(4095), (2, 2047));

        assert_eq!(offset_to_chunk(4096), (3, 0));
        assert_eq!(offset_to_chunk(8191), (3, 4095));

        assert_eq!(offset_to_chunk(8192), (4, 0));
    }

    #[test]
    fn test_chunk_index_to_size() {
        assert_eq!(chunk_index_to_size(0), FIRST_CHUNK_SIZE_BYTES);
        assert_eq!(chunk_index_to_size(1), FIRST_CHUNK_SIZE_BYTES);
        assert_eq!(chunk_index_to_size(2), FIRST_CHUNK_SIZE_BYTES * 2);
        assert_eq!(chunk_index_to_size(3), FIRST_CHUNK_SIZE_BYTES * 4);
        assert_eq!(chunk_index_to_size(4), FIRST_CHUNK_SIZE_BYTES * 8);
        assert_eq!(chunk_index_to_size(5), FIRST_CHUNK_SIZE_BYTES * 16);
    }

    #[test]
    fn test_single_allocation_and_get() {
        let arena: ChunkedArena<8> = ChunkedArena::new();
        let (offset, ptr) = unsafe { arena.alloc_append(16) };
        assert_eq!(offset, 0);

        unsafe {
            ptr.cast::<[u8; 16]>().write([0xAB; 16]);
        }

        unsafe {
            let retrieved = arena.get(0);
            assert_eq!(*retrieved.cast::<[u8; 16]>(), [0xAB; 16]);
        }
    }

    #[test]
    fn test_multiple_allocations_stable_pointers() {
        let arena: ChunkedArena<8> = ChunkedArena::new();

        let (off1, ptr1) = unsafe { arena.alloc_append(8) };
        unsafe { ptr1.cast::<u64>().write(0x1111_1111) };

        let (off2, ptr2) = unsafe { arena.alloc_append(8) };
        unsafe { ptr2.cast::<u64>().write(0x2222_2222) };

        let (off3, ptr3) = unsafe { arena.alloc_append(8) };
        unsafe { ptr3.cast::<u64>().write(0x3333_3333) };

        assert_eq!(off1, 0);
        assert_eq!(off2, 8);
        assert_eq!(off3, 16);

        // Pointers from alloc_append remain valid after subsequent allocations.
        unsafe {
            assert_eq!(*ptr1.cast::<u64>(), 0x1111_1111);
            assert_eq!(*ptr2.cast::<u64>(), 0x2222_2222);
            assert_eq!(*ptr3.cast::<u64>(), 0x3333_3333);
            //
            // get() resolves to the same addresses.
            assert_eq!(arena.get(off1), ptr1);
            assert_eq!(arena.get(off2), ptr2);
            assert_eq!(arena.get(off3), ptr3);
        }
    }

    #[test]
    fn test_alloc_after_filling_chunk_must_not_return_dangling() {
        let arena: ChunkedArena<8> = ChunkedArena::new();

        // Fill chunk 0 exactly.
        let (_, _p1) = unsafe { arena.alloc_append(1024) };

        // Allocate again. Should land at start of chunk 1. Chunk 1 was never
        // allocated, so if the arena doesn't lazily allocate it, this returns
        // the sentinel dangling pointer.
        let (offset2, p2) = unsafe { arena.alloc_append(8) };
        assert_eq!(offset2, 1024);

        let dangling = std::ptr::NonNull::<u8>::dangling().as_ptr();
        assert_ne!(
            p2, dangling,
            "alloc_append returned a dangling pointer: chunk 1 was never allocated"
        );
    }

    #[test]
    fn test_zero_size_alloc_does_not_allocate_chunk() {
        let arena: ChunkedArena<8> = ChunkedArena::new();

        let (off, ptr) = unsafe { arena.alloc_append(0) };
        assert_eq!(off, 0);

        // Zero-size allocation returns dangling — no chunk materialized.
        assert_eq!(ptr, NonNull::<u8>::dangling().as_ptr());

        // get() for an offset that was never backed by a real allocation returns null.
        assert!(unsafe { arena.get(off) }.is_null());

        // A subsequent real allocation should work normally and return the same offset
        // since the zero-size alloc didn't advance into any chunk.
        let (off2, ptr2) = unsafe { arena.alloc_append(8) };
        assert_eq!(off2, 0);
        assert_ne!(ptr2, NonNull::<u8>::dangling().as_ptr());
        unsafe { ptr2.cast::<u64>().write(0xDEAD_BEEF) };
        assert_eq!(unsafe { *arena.get(off2).cast::<u64>() }, 0xDEAD_BEEF);
    }

    #[test]
    fn test_chunk_index_to_start_offset() {
        let mut offset = 0usize;
        for chunk_index in 0..MAX_CHUNKS {
            assert_eq!(offset, chunk_index_to_start_offset(chunk_index));
            offset = offset.checked_add(chunk_index_to_size(chunk_index)).unwrap();
        }
    }
}
