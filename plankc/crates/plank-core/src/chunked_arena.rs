use allocator_api2::alloc::{AllocError, Allocator, Global, handle_alloc_error};
use std::{alloc::Layout, cell::Cell, ptr::NonNull};

const MAX_CHUNKS: usize = 22;
const FIRST_CHUNK_SIZE_BYTES: usize = 1024;

const _MAX_BYTES_FITS_IN_U32: () =
    assert!(FIRST_CHUNK_SIZE_BYTES * 2usize.pow(MAX_CHUNKS as u32) == 2usize.pow(u32::BITS));

fn chunk_index_to_size(chunk_index: u32) -> usize {
    let size_exponent = chunk_index.saturating_sub(1);
    FIRST_CHUNK_SIZE_BYTES * 2usize.pow(size_exponent)
}

fn offset_to_chunk(offset: usize) -> (usize, usize) {
    if offset < FIRST_CHUNK_SIZE_BYTES {
        return (0, offset);
    }
    let first_chunk_size_multiples = offset / FIRST_CHUNK_SIZE_BYTES;
    let chunk_index = first_chunk_size_multiples.ilog2() + 1;
    let size_exponent = first_chunk_size_multiples.ilog2();
    let chunk_start_offset = FIRST_CHUNK_SIZE_BYTES * 2usize.pow(size_exponent);
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
    next_free_offset: Cell<u32>,
    chunks: [Cell<NonNull<u8>>; MAX_CHUNKS],
    alloc: A,
}

impl<const ALIGN: usize> ChunkedArena<ALIGN> {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self::new_in(Global)
    }
}

impl<const ALIGN: usize, A: Allocator> ChunkedArena<ALIGN, A> {
    pub fn new_in(alloc: A) -> Self {
        const { assert!(ALIGN > 0 && ALIGN.is_power_of_two(), "invalid alignment") };
        const { assert!(FIRST_CHUNK_SIZE_BYTES.is_multiple_of(ALIGN), "alignment too large") };
        let chunks: [Cell<NonNull<u8>>; _] = [const { Cell::new(NonNull::dangling()) }; MAX_CHUNKS];
        let layout = chunk_layout(0, ALIGN);
        let first_chunk =
            alloc.allocate(layout).unwrap_or_else(|AllocError| handle_alloc_error(layout));
        chunks[0].set(first_chunk.cast());

        Self { next_free_offset: Cell::new(0), chunks, alloc }
    }

    /// Allocate `size` bytes of append-only storage.
    ///
    /// Returns the stable offset and a write pointer into the arena.
    /// The returned pointer is `ALIGN`-aligned and the offset is a multiple of `ALIGN`;
    /// both remain valid for the lifetime of the arena regardless of subsequent allocations.
    ///
    /// # Safety
    ///
    /// The caller must:
    /// - Pass a `size` that is a multiple of `ALIGN`.
    /// - Fully initialize `size` bytes at the returned pointer before the arena is dropped or
    ///   before the returned offset is used with [`get`](Self::get).
    pub unsafe fn alloc_append(&self, size: usize) -> (u32, *mut u8) {
        debug_assert!(size.is_multiple_of(ALIGN));

        let mut next_free_offset = self.next_free_offset.get();
        let (chunk_index, mut chunk_rel_offset) = offset_to_chunk(next_free_offset as usize);
        let mut chunk_index = chunk_index as u32;
        let mut remaining = chunk_index_to_size(chunk_index) - chunk_rel_offset;

        while size > remaining {
            if chunk_index as usize >= MAX_CHUNKS - 1 {
                panic!("attempting to allocate more than `MAX_CHUNKS`");
            }
            next_free_offset += remaining as u32;
            chunk_index += 1;
            chunk_rel_offset = 0;
            remaining = chunk_index_to_size(chunk_index);

            let layout = chunk_layout(chunk_index, ALIGN);
            let chunk =
                self.alloc.allocate(layout).unwrap_or_else(|AllocError| handle_alloc_error(layout));
            self.chunks[chunk_index as usize].set(chunk.cast());
        }

        // We know `size as u32` can't overflow `u32` because the `size > remaining` should've
        // errored first by running out of chunks if it didn't fit.
        self.next_free_offset.set(next_free_offset + size as u32);

        let base = self.chunks[chunk_index as usize].get();
        let write_ptr = unsafe { base.as_ptr().byte_add(chunk_rel_offset) };
        (next_free_offset, write_ptr)
    }

    /// Resolve a previously returned offset to a stable pointer.
    ///
    /// # Safety
    /// Requires `offset` to be derived from `alloc_append` called on the same struct.
    /// Furthermore data pointed to by the returned pointer which is part of the original
    /// allocation *MUST NOT* by mutated.
    pub unsafe fn get(&self, offset: u32) -> *const u8 {
        let (chunk_index, rel_offset) = offset_to_chunk(offset as usize);
        let base = self.chunks[chunk_index].get().as_ptr();
        unsafe { base.byte_add(rel_offset) }
    }
}

impl<const ALIGN: usize, A: Allocator> Drop for ChunkedArena<ALIGN, A> {
    fn drop(&mut self) {
        let (last_chunk_index, _) = offset_to_chunk(self.next_free_offset.get() as usize);
        for i in 0..=last_chunk_index {
            let layout = chunk_layout(i as u32, ALIGN);
            unsafe {
                let ptr = self.chunks[i].get();
                self.alloc.deallocate(ptr, layout)
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
    fn single_allocation_and_get() {
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
    fn multiple_allocations_stable_pointers() {
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
}
