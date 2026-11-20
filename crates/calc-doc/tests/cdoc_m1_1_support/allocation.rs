//! Per-thread allocation measurements for the acceptance tests.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static ALLOCATED: Cell<u64> = const { Cell::new(0) };
    static LARGEST: Cell<u64> = const { Cell::new(0) };
}

fn count(bytes: usize) {
    let _ = ALLOCATED.try_with(|total| total.set(total.get() + bytes as u64));
    let _ = LARGEST.try_with(|largest| largest.set(largest.get().max(bytes as u64)));
}

/// The bytes this thread has asked the allocator for so far (frees are not subtracted).
pub fn allocated() -> u64 {
    ALLOCATED.with(Cell::get)
}

/// The largest single allocation or reallocation, in bytes, this thread has asked for since the
/// last [`reset_largest`].
pub fn largest() -> u64 {
    LARGEST.with(Cell::get)
}

/// Starts [`largest`] again from 0.
pub fn reset_largest() {
    LARGEST.with(|largest| largest.set(0));
}

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;
