//! The heap's record of function hot swaps (BEP-087).
//!
//! A redirect lives on the target `Function`, which the VM reads on the call
//! path. A compile-time target is not walked by the collector, and a
//! replacement may be reachable only through its redirect, so this table is
//! what makes an occupied redirect a GC root and what repoints it after a
//! collection moves the replacement.

use std::{collections::HashMap, sync::Mutex};

use bex_vm_types::{HeapPtr, Object};

use crate::{BexHeap, HeapGuard};

#[derive(Clone, Copy)]
struct Installed {
    target: HeapPtr,
    replacement: HeapPtr,
}

/// A swap the engine prepared, waiting for its park.
#[derive(Clone, Copy)]
struct Pending {
    target: HeapPtr,
    /// `None` restores the function as built.
    replacement: Option<HeapPtr>,
}

#[derive(Default)]
struct Entries {
    installed: Vec<Installed>,
    pending: Vec<Pending>,
    /// The last installed patch that built changed clients: a later patch
    /// that changes no client since reuses their values instead of building
    /// them again.
    clients: Option<HeapPtr>,
}

#[derive(Default)]
pub(crate) struct HotswapTable(Mutex<Entries>);

impl HotswapTable {
    fn entries(&self) -> std::sync::MutexGuard<'_, Entries> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn redirect_of(target: HeapPtr) -> &'static bex_vm_types::types::HotswapRedirect {
    // SAFETY: a table entry's target is rooted, and the caller is parked or
    // holds a permit, so the object stays live and in place.
    match unsafe { target.get() } {
        Object::Function(function) => &function.hotswap_redirect,
        other => unreachable!("a hot swap target is a function, found {other:?}"),
    }
}

impl BexHeap {
    /// Queue a swap for the next [`Self::hotswap_apply_pending`]. Queued
    /// pointers are roots, so they survive a collection before the park.
    /// `replacement` of `None` restores the target's own body.
    pub fn hotswap_request(&self, target: HeapPtr, replacement: Option<HeapPtr>) {
        self.hotswap.entries().pending.push(Pending {
            target,
            replacement,
        });
    }

    /// Turn hot swapping on for VMs created from now on: their calls consult
    /// redirects.
    pub fn hotswap_enable(&self) {
        self.hotswap_on
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether any package can be swapped, so calls must consult redirects.
    pub fn hotswap_on(&self) -> bool {
        self.hotswap_on.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// The last installed patch that built changed clients.
    pub fn hotswap_clients(&self) -> Option<HeapPtr> {
        self.hotswap.entries().clients
    }

    /// Record `patch` as the one holding the current clients, or none when
    /// every client is as built.
    pub fn hotswap_set_clients(&self, patch: Option<HeapPtr>) {
        self.hotswap.entries().clients = patch;
    }

    /// Every function with a swap installed or queued, so a swap that leaves
    /// one as built can restore it.
    pub fn hotswap_swapped(&self) -> Vec<HeapPtr> {
        let entries = self.hotswap.entries();
        let mut swapped: Vec<HeapPtr> =
            entries.installed.iter().map(|entry| entry.target).collect();
        for entry in &entries.pending {
            if !swapped.contains(&entry.target) {
                swapped.push(entry.target);
            }
        }
        swapped
    }

    /// Install everything queued, oldest first, so racing installs are
    /// last-writer-wins in queue order. The `parked` guard is the proof that
    /// no VM is reading a redirect.
    pub fn hotswap_apply_pending(&self, _parked: &HeapGuard<'_>) {
        let mut entries = self.hotswap.entries();
        let pending = std::mem::take(&mut entries.pending);
        for Pending {
            target,
            replacement,
        } in pending
        {
            entries.installed.retain(|entry| entry.target != target);
            if let Some(replacement) = replacement {
                entries.installed.push(Installed {
                    target,
                    replacement,
                });
            }
            redirect_of(target).set(replacement);
        }
    }

    /// Every heap object an installed or queued swap keeps alive. Compile-time
    /// objects never move and are not collected, so they are left out.
    pub fn hotswap_roots(&self) -> Vec<HeapPtr> {
        let entries = self.hotswap.entries();
        let installed = entries
            .installed
            .iter()
            .flat_map(|entry| [entry.target, entry.replacement]);
        let pending = entries
            .pending
            .iter()
            .flat_map(|entry| std::iter::once(entry.target).chain(entry.replacement));
        installed
            .chain(pending)
            .chain(entries.clients)
            .filter(|ptr| !self.is_compile_time_ptr(*ptr))
            .collect()
    }

    /// Follow a collection: repoint every entry, then rewrite each target's
    /// redirect, since the replacement, the target, or both may have moved.
    pub fn hotswap_forward(&self, _parked: &HeapGuard<'_>, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        let forward = |ptr: HeapPtr| forwarding.get(&ptr).copied().unwrap_or(ptr);
        let mut entries = self.hotswap.entries();
        let entries = &mut *entries;
        entries.clients = entries.clients.map(forward);
        for entry in &mut entries.pending {
            entry.target = forward(entry.target);
            entry.replacement = entry.replacement.map(forward);
        }
        for entry in &mut entries.installed {
            entry.target = forward(entry.target);
            entry.replacement = forward(entry.replacement);
            redirect_of(entry.target).set(Some(entry.replacement));
        }
    }
}
