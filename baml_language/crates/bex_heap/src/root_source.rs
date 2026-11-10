//! Where a collection finds its roots.
//!
//! A [`RootSource`] reports precise roots (pointers known to name objects)
//! and conservative ranges (memory, such as a parked fiber's stack, in which
//! any word might be a pointer). A conservative word keeps alive the object
//! whose bytes contain it, wherever in the object it points; a word that
//! falls in no allocated cell is ignored.

use std::ops::Range;

use bex_vm_types::HeapPtr;

pub trait RootSource {
    /// Report every precise root.
    fn precise_roots(&mut self, visit: &mut dyn FnMut(HeapPtr));

    /// Report every range of words to scan conservatively. Each range is
    /// word-aligned, readable for the whole collection, and not written
    /// while it is scanned.
    fn conservative_ranges(&mut self, _visit: &mut dyn FnMut(Range<*const usize>)) {}
}

/// No roots at all.
pub struct NoRoots;

impl RootSource for NoRoots {
    fn precise_roots(&mut self, _visit: &mut dyn FnMut(HeapPtr)) {}
}

/// A list of precise roots.
pub struct PreciseRoots<'a>(pub &'a [HeapPtr]);

impl RootSource for PreciseRoots<'_> {
    fn precise_roots(&mut self, visit: &mut dyn FnMut(HeapPtr)) {
        self.0.iter().copied().for_each(visit);
    }
}

/// Ranges of words to scan conservatively, for instance a fiber stack from
/// its saved stack pointer to its base.
pub struct ConservativeRanges<'a>(pub &'a [Range<*const usize>]);

impl RootSource for ConservativeRanges<'_> {
    fn precise_roots(&mut self, _visit: &mut dyn FnMut(HeapPtr)) {}

    fn conservative_ranges(&mut self, visit: &mut dyn FnMut(Range<*const usize>)) {
        self.0.iter().cloned().for_each(visit);
    }
}

/// Both of two sources.
pub struct BothRoots<A, B>(pub A, pub B);

impl<A: RootSource, B: RootSource> RootSource for BothRoots<A, B> {
    fn precise_roots(&mut self, visit: &mut dyn FnMut(HeapPtr)) {
        self.0.precise_roots(visit);
        self.1.precise_roots(visit);
    }

    fn conservative_ranges(&mut self, visit: &mut dyn FnMut(Range<*const usize>)) {
        self.0.conservative_ranges(visit);
        self.1.conservative_ranges(visit);
    }
}

impl<T: RootSource + ?Sized> RootSource for &mut T {
    fn precise_roots(&mut self, visit: &mut dyn FnMut(HeapPtr)) {
        (**self).precise_roots(visit);
    }

    fn conservative_ranges(&mut self, visit: &mut dyn FnMut(Range<*const usize>)) {
        (**self).conservative_ranges(visit);
    }
}
