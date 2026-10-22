//! Type-directed encoding of host values for the engine's inbound wire.
//!
//! A host hands the engine a value for a slot the program declares: a
//! parameter, a class field, an element, a callback's result. This crate
//! walks the host value against that declared type and produces the exact
//! wire value the slot takes, so the engine only has to check and hydrate it.
//! Every conversion a language allows (an `int` where a `float` is declared, a
//! string naming an enum variant, a dictionary standing in for a class) is
//! decided here, on the host's side, by the host's own rules ([`Rules`]).
//!
//! One encode has three phases, each with one job:
//!
//! 1. **Read.** The host value is read once into a snapshot. This is the only
//!    phase that runs host code, so it is the only phase a host exception can
//!    come from.
//! 2. **Plan.** The snapshot is planned against the declared type. Planning
//!    is pure: it has no access to the host, so a union can try every member
//!    and a generic class can be solved and restated with nothing to undo.
//! 3. **Commit.** The plan is measured against the wire's nesting limit,
//!    then lowered to the wire through [`Host`], which registers each shipped
//!    callable and clones each shipped handle exactly once. What it creates
//!    is recorded in an [`Owners`] guard that releases everything unless the
//!    caller keeps it once the engine has the request.
//!
//! Every node leaves with its concrete type on the wire. A list, map, class or
//! callable node states it in `value_type`: the slot's type where the slot is
//! realized, the type the value itself has where the slot fixes none. A
//! scalar, null, enum variant, media value or handle implies its type and
//! carries nothing. That is the whole contract between this encoder and the
//! engine, which builds exactly the value stated and checks it against the
//! slot.

use std::collections::HashMap;

use baml_type::{Name, ParamTy};
use bridge_ctypes::{
    ARGUMENT_DEPTH, RESULT_DEPTH, WIRE_NESTING_LIMIT,
    baml_bridge::cffi::{InboundMapEntry, InboundValue, inbound_map_entry::Key},
    inbound_value_depth,
};

mod commit;
mod completion;
mod error;
mod facts;
mod host;
pub mod js;
mod plan;
mod read;
mod rules;

pub use completion::{Encoded, bridge_failure, raised};
pub use error::{
    EncodeError, EncodeErrorKind, Error, FactsError, FaultKind, PathSegment, Root, UnfitKind,
};
pub use facts::{ClassFacts, EnumFacts, Facts, FunctionSig, Param};
pub use host::{
    Children, Entries, Fields, Head, Host, HostShape, HostValue, LeafKind, LeafWire, Owner, Scalar,
    Shape, SlotTy, StatedTy,
};
pub use rules::Rules;

use crate::{
    error::EncodeErrorKind as Kind,
    plan::{Node, Planner},
    read::NODE_BUDGET,
};

/// Everything one request created on the host's side. Released on drop
/// unless [`keep`](Self::keep) is called once the engine has accepted the
/// request, at which point the decoder owns the handles and the engine the
/// registrations.
#[must_use = "an encode's owners are released when this is dropped; call `keep` once the engine has the request"]
pub struct Owners<'h, V: HostValue, H: Host<V>> {
    host: &'h H,
    created: Vec<Owner>,
    _value: std::marker::PhantomData<V>,
}

impl<'h, V: HostValue, H: Host<V>> Owners<'h, V, H> {
    pub fn new(host: &'h H) -> Self {
        Owners {
            host,
            created: Vec::new(),
            _value: std::marker::PhantomData,
        }
    }

    /// The engine has the request: what was created is its now.
    pub fn keep(mut self) {
        self.created.clear();
    }

    /// How many owners this request has created so far.
    pub fn created(&self) -> usize {
        self.created.len()
    }
}

impl<V: HostValue, H: Host<V>> Drop for Owners<'_, V, H> {
    fn drop(&mut self) {
        for owner in self.created.drain(..) {
            self.host.release(owner);
        }
    }
}

/// A call's explicit type arguments on the wire, in the order given: each
/// runtime declaration they name referenced afresh, one reference per
/// occurrence, so the host keeps the rows it holds and the engine takes
/// these.
pub fn state_type_args<V: HostValue, H: Host<V>>(
    type_args: &[(String, StatedTy)],
    owners: &mut Owners<'_, V, H>,
) -> Result<Vec<bridge_ctypes::baml_bridge::cffi::BamlTyArg>, V::Error> {
    type_args
        .iter()
        .map(|(name, ty)| {
            Ok(bridge_ctypes::baml_bridge::cffi::BamlTyArg {
                type_var: name.clone(),
                type_value: Some(commit::referenced(ty, owners.host, &mut owners.created)?),
            })
        })
        .collect()
}

/// Encode a function's arguments against its signature.
///
/// `arguments` are `(declared parameter name, value)`; `type_args` are the
/// caller's explicit type-parameter bindings by name. A type variable the
/// caller did not bind stays open and the value states its type. Names and
/// arity are the engine's to judge: an argument no parameter declares goes
/// as the type its value has, and the engine reports it. Every argument is
/// read and planned before any is committed, so a refusal creates nothing.
pub fn encode_arguments<V: HostValue, H: Host<V>>(
    facts: &Facts,
    signature: &FunctionSig,
    type_args: &HashMap<Name, StatedTy>,
    arguments: Vec<(String, V)>,
    owners: &mut Owners<'_, V, H>,
) -> Result<Vec<InboundMapEntry>, Error<V::Error>> {
    // The caller's type arguments are realized types, in stated form: a host
    // never names a type variable, so no host-chosen variable reaches a solve.
    let resolve = |param: &ParamTy| {
        type_args
            .get(&Name::new(param.as_str()))
            .map(|ty| facts.stated_form(SlotTy::from(ty)))
    };
    let mut budget = NODE_BUDGET;
    let mut trees = Vec::with_capacity(arguments.len());
    for (name, value) in arguments {
        let slot = signature
            .params
            .iter()
            .find(|param| param.name == name)
            .map_or(SlotTy::Unknown, |param| {
                facts::substitute(&param.ty, &resolve)
            });
        let tree = read::read(PathSegment::Argument(name.clone()), value, &mut budget)?;
        trees.push((name, slot, tree));
    }
    let mut planner = Planner::new(facts);
    let mut nodes = Vec::with_capacity(trees.len());
    for (name, slot, tree) in &trees {
        let node = planner
            .plan_root(tree, slot)
            .map_err(|failure| failure.into_error(PathSegment::Argument(name.clone())))?;
        nodes.push((name, node));
    }
    nodes
        .into_iter()
        .map(|(name, node)| {
            let value = ship(&node, ARGUMENT_DEPTH, owners)
                .map_err(|error| rooted(error, PathSegment::Argument(name.clone())))?;
            Ok(InboundMapEntry {
                key: Some(Key::StringKey(name.clone())),
                value: Some(value),
            })
        })
        .collect()
}

/// `error` with `segment` as its root where it has none: a failure at
/// commit knows no path, only the argument or result it was about.
fn rooted<E>(error: Error<E>, segment: PathSegment) -> Error<E> {
    match error {
        Error::Value(mut error) if error.path.is_empty() => {
            error.path.push(segment);
            Error::Value(error)
        }
        other => other,
    }
}

/// Encode one value against `slot`, rooted at `root`.
pub fn encode<V: HostValue, H: Host<V>>(
    facts: &Facts,
    value: V,
    slot: &SlotTy,
    root: Root,
    owners: &mut Owners<'_, V, H>,
) -> Result<InboundValue, Error<V::Error>> {
    let depth = match root {
        Root::Argument(_) => ARGUMENT_DEPTH,
        Root::Result => RESULT_DEPTH,
    };
    let segment = PathSegment::from(root);
    let mut budget = NODE_BUDGET;
    let tree = read::read(segment.clone(), value, &mut budget)?;
    let mut planner = Planner::new(facts);
    let node = planner
        .plan_root(&tree, slot)
        .map_err(|failure| failure.into_error(segment.clone()))?;
    ship(&node, depth, owners).map_err(|error| rooted(error, segment))
}

/// Commit `node` and measure the wire it makes against the decoder's limit:
/// once from the plan, before anything is created, and once exactly, after.
fn ship<V: HostValue, H: Host<V>>(
    node: &Node<'_, V, StatedTy>,
    enclosing: usize,
    owners: &mut Owners<'_, V, H>,
) -> Result<InboundValue, Error<V::Error>> {
    let too_deep = || {
        Error::Value(EncodeError {
            path: Vec::new(),
            kind: Box::new(Kind::Fault(FaultKind::TooDeep {
                limit: WIRE_NESTING_LIMIT,
            })),
        })
    };
    if enclosing + commit::depth(node) > WIRE_NESTING_LIMIT {
        return Err(too_deep());
    }
    let value = commit::commit(node, owners.host, &mut owners.created).map_err(Error::Commit)?;
    if enclosing + inbound_value_depth(&value) > WIRE_NESTING_LIMIT {
        return Err(too_deep());
    }
    Ok(value)
}

/// What a host does with a callback's result or thrown value once encoded
/// against the declared type: the one mapping every bridge uses.
#[derive(Debug)]
pub enum Completion<E> {
    /// The value fits: complete the call with it.
    Wire(InboundValue),
    /// The value is not a member of the declared type: the callable broke its
    /// contract, a `baml.panics.HostContractViolation`.
    ContractViolation(EncodeError),
    /// The callback's own code raised while the value was read: a thrown host
    /// error, as any exception the callback raises.
    HostError(E),
    /// The bridge could not carry a value the callable produced in good
    /// faith: a bridge failure, `baml.panics.SdkPanic`.
    BridgeFailure(BridgeFault<E>),
}

/// Why the bridge could not carry a value: a fault the walker found (too
/// deep, a declaration the facts do not reach) or the host's own exception
/// while a decided plan was committed (a handle that could not be cloned).
#[derive(Debug)]
pub enum BridgeFault<E> {
    Value(EncodeError),
    Host(E),
}

impl<E> From<Result<InboundValue, Error<E>>> for Completion<E> {
    fn from(result: Result<InboundValue, Error<E>>) -> Self {
        match result {
            Ok(value) => Completion::Wire(value),
            Err(Error::Host(error)) => Completion::HostError(error),
            Err(Error::Commit(error)) => Completion::BridgeFailure(BridgeFault::Host(error)),
            Err(Error::Value(error)) => match *error.kind {
                Kind::Unfit(_) => Completion::ContractViolation(error),
                Kind::Fault(_) => Completion::BridgeFailure(BridgeFault::Value(error)),
            },
        }
    }
}

#[cfg(test)]
mod tests;
