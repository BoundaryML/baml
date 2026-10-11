Question
  What does a call-heavy loop cost when every call passes and returns heap
  objects?

Work
  `count` iterations of four small functions over two-field class
  instances: add (allocates), scale (allocates), swap (stores into a
  four-slot ring array and returns the evicted object) and pick (returns
  one of its arguments). Five calls and three allocations per iteration;
  the result is the final accumulator and two ring fields.

BAML path
  Call lowering with heap arguments and results, field reads, small
  allocations, an array store. Natively every argument passed by value is a
  handle clone (an Rc increment, and a decrement when the callee drops it;
  the class is acyclic, so it is never tracked);
  a tracing collector instead has to root every live handle across each
  call (shadow-stack pushes and pops). This is the worst case for both.

Boundary
  Only count and seed are prepared. The loop, its allocations and
  serializing the result are timed.

Evidence
  The oracle replays the loop in Python on tuples. Checks cover zero and
  one iteration, a ring wrap-around and the documented seeds.

Claim
  Per-call cost of heap-object traffic under the runtime's memory manager.
  Inlining is allowed; inspect the generated code before claiming calls.

References
  https://llvm.org/docs/GarbageCollection.html
