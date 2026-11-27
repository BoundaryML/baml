//! `for` loops over arrays.
//!
//! MIR drives every `for` through the iterator interfaces: `iter()` on the
//! collection, then `next()` on the iterator until it answers `Done`. When
//! the collection is an array, native code needs neither the iterator object
//! nor the dispatch: the iterator is the array and an index, and the answer
//! of `next` is a flag and an element.
//!
//! This module finds the locals of a body that are such iterators and
//! answers. Any use of one that the loop shape does not explain is rejected
//! by the ordinary checks, because neither local has a type to them.

use std::collections::HashMap;

use baml_compiler2_hir_ty::layout::lang_class;
use baml_compiler2_mir::{Local, Operand, Place, Terminator, TypeTest};

use crate::types::{Body, NativeTy, operand_ty};

const ITER_METHOD: &str = "iter";
const NEXT_METHOD: &str = "next";

#[derive(Default)]
pub(crate) struct ArrayLoops<'db> {
    /// The result of `iter()` on an array, with the array's element type.
    iterators: HashMap<Local, NativeTy<'db>>,
    /// The result of `next()` on such an iterator, with the element type.
    items: HashMap<Local, NativeTy<'db>>,
}

/// The local an operand reads, when it reads a whole local.
pub(crate) fn operand_local(operand: &Operand<'_>) -> Option<Local> {
    match operand {
        Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => Some(*local),
        _ => None,
    }
}

/// The receiver and destination of a call of `method` with no other
/// arguments, no handler and no trace.
fn receiver_call<'a, 'db>(
    terminator: &'a Terminator<'db>,
    method: &str,
) -> Option<(&'a Operand<'db>, Local)> {
    match terminator {
        Terminator::VirtualCall {
            has_trace: false,
            method: called,
            args,
            ntypeargs: 0,
            destination: Place::Local(destination),
            unwind: None,
            ..
        } if called == method => match &args[..] {
            [receiver] => Some((receiver, *destination)),
            _ => None,
        },
        _ => None,
    }
}

impl<'db> ArrayLoops<'db> {
    pub(crate) fn of(db: &'db dyn baml_compiler2_mir::Db, body: &Body<'_, 'db>) -> Self {
        let mut loops = Self::default();
        // A local defined twice with different element types is not a loop
        // local of either.
        let mut conflicting = Vec::new();
        let mut record =
            |map: &mut HashMap<Local, NativeTy<'db>>, local, element: NativeTy<'db>| {
                if map.get(&local).is_some_and(|known| *known != element) {
                    conflicting.push(local);
                }
                map.insert(local, element);
            };
        let terminators = || {
            body.blocks
                .iter()
                .filter_map(|block| block.terminator.as_ref())
        };
        for terminator in terminators() {
            if let Some((receiver, iterator)) = receiver_call(terminator, ITER_METHOD)
                && only_stepped(body, iterator)
                && let Ok(NativeTy::Array(element)) = operand_ty(db, body, receiver)
            {
                record(&mut loops.iterators, iterator, *element);
            }
        }
        for terminator in terminators() {
            if let Some((receiver, item)) = receiver_call(terminator, NEXT_METHOD)
                && let Some(element) =
                    operand_local(receiver).and_then(|it| loops.iterators.get(&it))
            {
                let element = element.clone();
                record(&mut loops.items, item, element);
            }
        }
        for local in conflicting {
            loops.iterators.remove(&local);
            loops.items.remove(&local);
        }
        loops
    }

    /// The element type, when `local` is an array iterator.
    pub(crate) fn iterator(&self, local: Local) -> Option<&NativeTy<'db>> {
        self.iterators.get(&local)
    }

    /// The element type, when `local` is what `next()` answered.
    pub(crate) fn item(&self, local: Local) -> Option<&NativeTy<'db>> {
        self.items.get(&local)
    }

    /// `array.iter()` into an iterator local: the array and the local.
    pub(crate) fn start<'a>(
        &self,
        terminator: &'a Terminator<'db>,
    ) -> Option<(&'a Operand<'db>, Local)> {
        receiver_call(terminator, ITER_METHOD)
            .filter(|(_, iterator)| self.iterators.contains_key(iterator))
    }

    /// `iterator.next()` into an item local: the iterator and the item.
    pub(crate) fn step(&self, terminator: &Terminator<'db>) -> Option<(Local, Local)> {
        let (receiver, item) = receiver_call(terminator, NEXT_METHOD)?;
        let iterator = operand_local(receiver)?;
        (self.iterators.contains_key(&iterator) && self.items.contains_key(&item))
            .then_some((iterator, item))
    }
}

/// Whether every mention of `iterator` is the `iter()` that makes it or a
/// `next()` on it. One that goes anywhere else is an iterator object.
fn only_stepped(body: &Body<'_, '_>, iterator: Local) -> bool {
    use std::fmt::Write as _;

    let name = format!("{iterator:?}");
    let mut mentions = 0;
    let mut explained = 0;
    let mut code = String::new();
    for block in &body.blocks {
        code.clear();
        for statement in &block.statements {
            let _ = write!(code, "{:?};", statement.kind);
        }
        if let Some(terminator) = &block.terminator {
            let _ = write!(code, "{terminator:?}");
            let made = receiver_call(terminator, ITER_METHOD)
                .is_some_and(|(_, destination)| destination == iterator);
            let stepped = receiver_call(terminator, NEXT_METHOD)
                .is_some_and(|(receiver, _)| operand_local(receiver) == Some(iterator));
            explained += usize::from(made) + usize::from(stepped);
        }
        mentions += code.matches(&name).count();
    }
    mentions == explained
}

/// Whether a type test asks "is this the end of the iteration".
pub(crate) fn is_done_test<'db>(db: &'db dyn baml_compiler2_mir::Db, test: &TypeTest<'db>) -> bool {
    match test {
        TypeTest::Class { class, args } if args.is_empty() => {
            lang_class(db, baml_base::LangPackage::Baml, &["iter"], "Done") == Some(*class)
        }
        _ => false,
    }
}
