# Code added while the program runs

Status: not designed. Notes on what `reflect.Package.compile` will need from the runtime, so they are not lost.

## Interface rules have to be removed when their package dies

An `ImplRule` added at run time is not a heap object (`../memory.md`, "Objects"). It sits in the `dynamic_rules` list of the interface it implements, and that interface may belong to another package: a package can implement an interface of a package it depends on. So when a package made at run time is no longer reachable, the collector cannot free it by dropping its own objects. Every rule it declared has to come out of every interface's list, or a later call resolves to a function whose code is gone.

What that needs:

- A rule knows who declared it: `ImplRule.owner`.
- The collector knows a package is dead. A package is alive while any of its instances, functions, types or class objects is reachable, so each of those has to keep its `PackageObject` alive, and a dead `PackageObject` is what triggers the cleanup.
- A way to find the rules of a dead package without walking every interface: the package keeps the list of interfaces it added rules to.
- The removal has to be safe against a worker that is resolving a call through the same list. The list is only ever appended to while the program runs normally; removal happens with the world stopped.
- A `RawFunctionObject` is a heap object, also when the function was made at run time, and the code it points at has to be loaded and later unloaded. So a rule added at run time holds heap pointers: the collector visits the `function` of each method of every rule in an interface's `dynamic_rules`, and removes the rules of a dead package. How code is loaded and unloaded is not designed.

## Caches

Any cache of a resolved interface call (per call site, or per receiver class and method) has to be dropped when a rule is added or removed. One counter per interface, bumped on either, is enough for a cache to check.

## Types that name a class made at run time

A `RealizedTy` names a class or an interface by the address of its object (`../memory.md`, "Objects"). While every class object is in the executable's data that address never moves and nothing has to trace it. A class made at run time is a heap object, so the collector then has to visit the addresses inside every type object's `RealizedTy`, and inside every rule and template that names one.
