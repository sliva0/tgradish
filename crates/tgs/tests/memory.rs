//! How much memory reading art as pixel art and reducing it take beside
//! the art, counted by an allocator of its own: a binary of its own, with
//! one test, so nothing else allocates while it counts.

// a global allocator can only be written with unsafe code
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use tgradish_tgs::frames::{Animation, Frame};
use tgradish_tgs::normalise::{Options, normalise};
use tgradish_tgs::reduce::Reduction;

/// The system's allocator, counting the bytes allocated now and the most
/// at once since [`peak_during`] last started counting.
struct Counting;

static NOW: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn grew(bytes: usize) {
    let now = NOW.fetch_add(bytes, Ordering::SeqCst) + bytes;
    PEAK.fetch_max(now, Ordering::SeqCst);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: as the caller promised for this layout
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            grew(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: as the caller promised: allocated here, with this layout
        unsafe { System.dealloc(pointer, layout) };
        NOW.fetch_sub(layout.size(), Ordering::SeqCst);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: as the caller promised for the pointer, layout and size
        let moved = unsafe { System.realloc(pointer, layout, size) };
        if !moved.is_null() {
            match size.checked_sub(layout.size()) {
                Some(more) => grew(more),
                None => {
                    NOW.fetch_sub(layout.size() - size, Ordering::SeqCst);
                }
            }
        }
        moved
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// What `work` returns, and the most bytes it held at once beside what
/// was allocated before it started.
fn peak_during<T>(work: impl FnOnce() -> T) -> (T, usize) {
    let start = NOW.load(Ordering::SeqCst);
    PEAK.store(start, Ordering::SeqCst);
    let done = work();
    (done, PEAK.load(Ordering::SeqCst) - start)
}

/// 48 frames of 128x128 art at 4 input pixels an art pixel, of a dozen
/// opaque colours, no two frames alike: 3 MiB of RGBA, a few dozen KiB of
/// cells.
fn art() -> Animation {
    let (side, scale) = (128u32, 4u32);
    let mut seed = 0x2545_f491_4f6c_dd1du64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let frames = (0..48)
        .map(|_| {
            let cells = side / scale;
            let colours: Vec<u8> = (0..cells * cells).map(|_| (next() % 12) as u8).collect();
            let mut rgba = Vec::with_capacity((side * side * 4) as usize);
            for y in 0..side {
                for x in 0..side {
                    let colour = colours[(y / scale * cells + x / scale) as usize];
                    rgba.extend([colour * 20, 255 - colour * 9, 40, 255]);
                }
            }
            Frame { rgba, duration: Duration::from_millis(40) }
        })
        .collect();
    Animation::new(side, side, frames).unwrap()
}

#[test]
fn reads_and_reduces_art_in_little_more_than_it_takes() {
    let animation = art();
    let input: usize = animation.frames().iter().map(|frame| frame.rgba.len()).sum();

    // packing every frame beside the art the caller still held took as
    // much again as the art; each frame now goes once it is packed
    let (normalised, beside) = peak_during(|| normalise(animation, &Options::default()));
    let (anim, _) = normalised.unwrap();
    assert!(beside < input / 4, "normalising took {beside} bytes beside {input}");
    assert_eq!((anim.width(), anim.height()), (32, 32));

    // snapping and downscaling worked on every input pixel of every frame,
    // as many bytes as the art again; now on the cells
    for reduction in [Reduction::SnapToGrid { scale: 8 }, Reduction::Downscale { factor: 0.5 }] {
        let (reduced, used) = peak_during(|| reduction.apply(&anim));
        assert!(reduced.unwrap().width() <= 16, "{reduction:?}");
        assert!(used < input / 8, "{reduction:?} took {used} bytes beside {input} of art");
    }
}
