// Generic function-call coverage, the INFERENCE variant (ns_generic_tests).
// Port of function_calls/customizable/test_generic_inference.py. Deviations:
// - Python's bare calls exercise ENGINE-side inbound inference. C++ has no
//   bare-call surface: template argument DEDUCTION solves each TypeVar from
//   the parameter's static type (identity(int64_t{5}) deduces T), and the
//   generated binding then sends that binding EXPLICITLY on the wire - so
//   engine-side inference is NOT exercised by any case in this file; each
//   case pins that the deduced-then-explicit form produces the same result.
// - Cases where deduction cannot work are compile errors and are skipped
//   with a comment at their Python position: return/body-only TypeVars called
//   bare, divergent-deduction joins (choose(5, "asdf") etc., where the engine
//   would union two conflicting occurrences), unbound generic instances
//   (a C++ class template instance always carries its type args), null/empty
//   actuals with no carrier type, and every negative TypeError case (variance
//   conflicts, uninferable vars, contradicted bindings) - all rejected by the
//   C++ compiler, not the engine.
// - `apply` is not emitted by sdkgen_cpp (callable-typed argument), so both
//   apply cases are skipped.
#include <baml_sdk.h>
#include <baml_test.h>

namespace generic_tests = baml_sdk::generic_tests;
using generic_tests::GenericBox;
using generic_tests::GenericPair;
using generic_tests::GenericRecursive;
using generic_tests::GenericTriple;
using generic_tests::NamedStatic;
using generic_tests::SomeEnum;
using generic_tests::StringIntPair;

// ===========================================================================
// SecA - single TypeVar deduced from one argument value
// ===========================================================================

BAML_TEST(identity_infers_primitives) {
  // T1/T2: T deduced from the value; identity returns it unchanged.
  BAML_ASSERT_EQ(generic_tests::identity(int64_t{5}), int64_t{5});
  BAML_ASSERT_EQ(generic_tests::identity(std::string("hi")), std::string("hi"));
  BAML_ASSERT_EQ(generic_tests::identity(true), true);
}

BAML_TEST(identity_infers_user_class) {
  // T3: T = StringIntPair, deduced from the instance value.
  const StringIntPair pair{"a", 1};
  BAML_ASSERT(generic_tests::identity(pair) == pair);
}

BAML_TEST(identity_infers_generic_instance) {
  // T4: a fully-bound GenericBox<int64_t> carries its type arg statically.
  const GenericBox<int64_t> box{5};
  BAML_ASSERT(generic_tests::identity(box) == box);

  const GenericBox<GenericBox<std::string>> nested{
      GenericBox<std::string>{"hello"}};
  BAML_ASSERT(generic_tests::identity(nested) == nested);
}

BAML_TEST(identity_async_infers) {
  // T5: the async path deduces identically.
  BAML_ASSERT_EQ(generic_tests::identity_async(int64_t{7}).get(), int64_t{7});
}

// test_identity_null_round_trips: skipped - a bare null carries no C++ type
// (identity(std::nullopt) deduces std::nullopt_t, which has no codec), so the
// host-only rust_type default is unreachable from C++.

// test_identity_unbound_generic_instance_round_trips: skipped - C++ cannot
// construct an UNBOUND generic instance; a class template instance always
// carries its type args, so the G2 host-only path is unreachable.

// ===========================================================================
// SecB - structural / container solving across one or more arguments
// ===========================================================================

BAML_TEST(make_triple_infers_multiple_typevars) {
  // T6: A=int (scalar), B=string (list element), C=bool (map value) - all
  // three deduced from differently-shaped arguments at once.
  const GenericTriple<int64_t, std::string, bool> t =
      generic_tests::make_triple(int64_t{1}, std::vector<std::string>{"a", "b"},
                                 std::map<std::string, bool>{{"k", true}});
  BAML_ASSERT_EQ(t.first, int64_t{1});
  BAML_ASSERT(t.second == (std::vector<std::string>{"a", "b"}));
  BAML_ASSERT(t.third == (std::map<std::string, bool>{{"k", true}}));
}

BAML_TEST(second_of_infers_from_nested_generic) {
  // T9: second_of<T>(p: GenericPair<int, T>) - T deduced from the pair's
  // second slot (first is pinned to int in the signature).
  BAML_ASSERT_EQ(
      generic_tests::second_of(GenericPair<int64_t, std::string>{1, "hi"}),
      std::string("hi"));
  const StringIntPair pair{"z", 9};
  const GenericPair<int64_t, StringIntPair> p{0, pair};
  BAML_ASSERT(generic_tests::second_of(p) == pair);
}

BAML_TEST(list_head_infers_from_recursive_generic) {
  // T11: GenericRecursive<T> bottoms out at next=nullopt; T deduced.
  const GenericRecursive<int64_t> linked{
      7, GenericRecursive<int64_t>{8, std::nullopt}};
  BAML_ASSERT_EQ(generic_tests::list_head(linked), int64_t{7});
}

BAML_TEST(extract_infers_four_typevars_from_nesting) {
  // T12: A,B,C,D deduced by walking the nested GenericPair instantiation.
  const GenericPair<GenericPair<int64_t, std::string>,
                    GenericPair<bool, double>>
      pair{
          GenericPair<int64_t, std::string>{1, "a"},
          GenericPair<bool, double>{true, 1.5},
      };
  BAML_ASSERT_EQ(generic_tests::extract(pair),
                 std::string("int | string | bool | float"));
}

// ===========================================================================
// SecC - union unification: one TypeVar across two argument positions
// ===========================================================================

BAML_TEST(choose_infers_unified_typevar) {
  // T14: both occurrences deduce the same T.
  BAML_ASSERT_EQ(generic_tests::choose(int64_t{5}, int64_t{6}), int64_t{5});
  BAML_ASSERT_EQ(generic_tests::choose(std::string("a"), std::string("b")),
                 std::string("a"));
}

// test_choose_infers_divergent_union: skipped - choose(5, "asdf") is a C++
// deduction conflict (T=int vs T=string), a compile error; the engine-side
// union join across two arguments is unreachable from C++.

// ===========================================================================
// SecD - partial binding: explicit seed for one TypeVar, deduce the rest
// ===========================================================================

BAML_TEST(make_triple_partial_explicit_then_infer) {
  // C2/T17: A seeded by an explicit template arg, B and C deduced from the
  // arguments (C++'s native partial-binding surface, the `_types=` analogue).
  const GenericTriple<int64_t, std::string, bool> t =
      generic_tests::make_triple<int64_t>(
          1, std::vector<std::string>{"x", "y"},
          std::map<std::string, bool>{{"k", true}});
  BAML_ASSERT_EQ(t.first, int64_t{1});
  BAML_ASSERT(t.second == (std::vector<std::string>{"x", "y"}));
  BAML_ASSERT(t.third == (std::map<std::string, bool>{{"k", true}}));
}

// ===========================================================================
// SecG/outbound - deduce T, return a generic over it
// ===========================================================================

BAML_TEST(wrap_infers_and_returns_generic) {
  // T29: wrap(5) deduces T=int and returns a GenericBox<int64_t>.
  const GenericBox<int64_t> w = generic_tests::wrap(int64_t{5});
  BAML_ASSERT_EQ(w.value, int64_t{5});
}

// ===========================================================================
// SecK - methods: class T from the receiver, method TypeVars deduced from args
// ===========================================================================

BAML_TEST(genericbox_pair_with_infers_method_typevar) {
  // T37: class T=int from the GenericBox<int64_t> receiver; method U=string
  // deduced from `other` (no explicit template arg).
  const GenericBox<int64_t> b{5};
  BAML_ASSERT_EQ(b.pair_with(std::string("hello world")),
                 std::string("int | string"));
}

BAML_TEST(generic_static_infers_own_typevar) {
  // T38: GenericBox.new<V>(value: V) - V deduced from the value. The class
  // must still be parameterized to name the static, so a class-level T
  // binding rides along (see the file header).
  const GenericBox<int64_t> box = GenericBox<int64_t>::new_(int64_t{5});
  BAML_ASSERT_EQ(box.value, int64_t{5});
}

BAML_TEST(named_static_infers_distinct_typevars) {
  // T39: NamedStatic.make<D,E>(d, e) - D=int, E=string deduced from the
  // args; arbitrary class A/B/C bindings ride along (C++ must parameterize
  // the class to name the static).
  BAML_ASSERT_EQ((NamedStatic<int64_t, int64_t, int64_t>::make(
                     int64_t{1}, std::string("x"))),
                 std::string("int | string"));
}

// ===========================================================================
// Out-of-scope / must-specify
// ===========================================================================

// test_union_concrete_sibling_absorbs_value_binds_rust_type: skipped - the
// $rust_type default requires sending NO binding for T; every C++ call sends
// T explicitly, so the host-only default is unreachable.
// test_union_null_actual_binds_rust_type: skipped - same, plus a bare null
// carries no C++ type to deduce from.
// test_return_only_var_still_requires_binding: skipped - a bare parse_as("42")
// does not compile in C++ (T is not deducible), so the engine rejection is
// unreachable.
// test_body_only_var_still_requires_binding: skipped - a bare one_type_arg()
// does not compile in C++ (no deducible template params).

// ===========================================================================
// SecJ - variance soundness: conflicting occurrences of one TypeVar
// ===========================================================================

// test_pair_invariant_list_conflict_rejects: skipped - pair([1,2], ["a","b"])
// is a C++ deduction conflict (compile error); the engine's variance
// rejection is unreachable.

BAML_TEST(pair_invariant_list_agree_binds) {
  // J9/G1: two invariant occurrences that AGREE deduce T=int.
  BAML_ASSERT_EQ(generic_tests::pair(std::vector<int64_t>{1, 2},
                                     std::vector<int64_t>{3, 4}),
                 std::string("int"));
}

// test_choose_union_outside_container_is_sound: skipped - choose(int[],
// string[]) is a C++ deduction conflict; the engine-formed union outside the
// container is unreachable.
// test_merge_invariant_map_value_conflict_rejects: skipped - deduction
// conflict (compile error).
// test_combine_invariant_class_arg_conflict_rejects: skipped - deduction
// conflict (compile error).
// test_glue_invariant_vs_covariant_conflict_rejects: skipped - deduction
// conflict (compile error).

BAML_TEST(glue_invariant_and_covariant_agree_binds) {
  // J11/G4: invariant (T==int) + covariant (int <: int) AGREE; T=int.
  BAML_ASSERT_EQ(generic_tests::glue(int64_t{1}, std::vector<int64_t>{2, 3}),
                 std::string("int"));
}

// test_two_typevar_union_is_uninferrable_rejects: skipped - two_in_union
// cannot deduce T and U from a bare call (compile error).

// ===========================================================================
// SecD n-ary covariant join, and SecB heterogeneous container element
// ===========================================================================

// test_triple_choose_three_covariant_join: skipped - triple_choose(5, "asdf",
// true) is a C++ deduction conflict; the engine's n-ary covariant join across
// separate arguments is unreachable.

// test_choose_divergent_generic_instances_union: skipped - choose(
// GenericBox<int64_t>, GenericBox<string>) is a C++ deduction conflict.

// ===========================================================================
// SecB - empty collections on a FREE function
// ===========================================================================

BAML_TEST(first_or_empty_list_round_trips_none) {
  // B7: in Python the empty list yields no evidence and T defaults to
  // rust_type. A C++ vector always carries an element type (T=int here), so
  // only the observable null return is pinned, not the rust_type default.
  BAML_ASSERT(!generic_tests::first_or(std::vector<int64_t>{}).has_value());
}

BAML_TEST(first_or_nonempty_infers_element) {
  // B7 twin: a non-empty list deduces the element and returns the head.
  BAML_ASSERT(generic_tests::first_or(std::vector<int64_t>{7, 8, 9}) ==
              std::optional<int64_t>{7});
}

BAML_TEST(values_of_empty_map_round_trips_empty_list) {
  // B9: as with first_or, the C++ map always carries its value type, so the
  // empty-collection rust_type default is not exercised; the observable
  // empty-list return is pinned.
  BAML_ASSERT(generic_tests::values_of(std::map<std::string, int64_t>{}) ==
              (std::vector<int64_t>{}));
}

BAML_TEST(values_of_nonempty_returns_values) {
  // B9 twin: a non-empty map returns its values.
  BAML_ASSERT(generic_tests::values_of(std::map<std::string, int64_t>{
                  {"a", 1}, {"b", 2}}) == (std::vector<int64_t>{1, 2}));
}

// ===========================================================================
// SecC - caller-specified & partial binding via template args
// ===========================================================================

// test_make_triple_partial_subscript_requires_full_arity: skipped - C++
// permits a partial explicit template-arg list with deduction of the rest
// (that IS the partial-binding surface here), so Python's host-side arity
// error has no counterpart.

BAML_TEST(make_triple_subscript_fully_bound) {
  // C3: every var seeded by explicit template args; each arg is checked
  // against its now-concrete formal at compile time.
  const GenericTriple<int64_t, std::string, bool> t =
      generic_tests::make_triple<int64_t, std::string, bool>(5, {"x"},
                                                             {{"k", true}});
  BAML_ASSERT_EQ(t.first, int64_t{5});
  BAML_ASSERT(t.second == (std::vector<std::string>{"x"}));
  BAML_ASSERT(t.third == (std::map<std::string, bool>{{"k", true}}));
}

// ===========================================================================
// SecE - must-specify: the explicit form succeeds
// ===========================================================================

BAML_TEST(one_type_arg_explicit_types_succeeds) {
  // E2: the body-only var supplied via the template arg reflects the type.
  BAML_ASSERT_EQ(generic_tests::one_type_arg<int64_t>(), std::string("int"));
}

BAML_TEST(parse_as_explicit_types_succeeds) {
  // E4: the return-only var bound via the template arg; the value parses.
  BAML_ASSERT_EQ(generic_tests::parse_as<int64_t>("42"), int64_t{42});
}

// ===========================================================================
// SecG - unbound generic instances: not expressible in C++
// ===========================================================================

// test_second_of_unbound_instance_recovers_field_type: skipped - C++ class
// template instances always carry their type args; an unbound GenericPair
// cannot be constructed.
// test_identity_nested_unbound_round_trips: skipped - same.

BAML_TEST(wrap_infers_and_returns_bound_generic) {
  // G4 (positive half): wrap(5) deduces T=int and the returned box equals
  // the bound literal. (bound vs unbound is inexpressible in C++ - every
  // instance is bound.)
  BAML_ASSERT(generic_tests::wrap(int64_t{5}) == (GenericBox<int64_t>{5}));
}

// ===========================================================================
// SecI - nullable param, enum round-trip
// ===========================================================================

BAML_TEST(maybe_id_present_value_infers) {
  // I1: the non-null arm of T? deduces against the int actual.
  BAML_ASSERT(generic_tests::maybe_id(std::optional<int64_t>{5}) ==
              std::optional<int64_t>{5});
}

// test_maybe_id_null_round_trips: skipped - maybe_id(std::nullopt) cannot
// deduce T (compile error); the rust_type default is unreachable.

BAML_TEST(identity_enum_round_trips) {
  // I3: an enum value rides through and round-trips. The C++ return type is
  // statically SomeEnum, so Python's isinstance assertion is inherent.
  const SomeEnum result = generic_tests::identity(SomeEnum::VARIANT);
  BAML_ASSERT(result == SomeEnum::VARIANT);
}

// ===========================================================================
// SecF - host-only object boundary
// ===========================================================================

// test_host_only_object_not_encodable_from_python: skipped - an arbitrary
// host struct has no baml::Codec specialization, so encoding it is a compile
// error rather than a runtime TypeError.

// ===========================================================================
// SecJ J13 - function-typed (host callable) arguments
// ===========================================================================

// test_apply_closure_poisons_typevars_must_specify: skipped - `apply` is not
// emitted by sdkgen_cpp (argument `f` has unsupported type callable<[T], R>).
// test_apply_closure_typevars_specified_succeeds: skipped - same.

// ===========================================================================
// SecL - methods: class T from the receiver
// ===========================================================================

BAML_TEST(genericbox_get_infers_class_var_from_receiver) {
  // L1: class T recovered from the receiver's type args.
  BAML_ASSERT_EQ((GenericBox<int64_t>{5}).get(), std::string("int"));
}

// test_genericbox_pair_with_unbound_receiver_recovers_class_var: skipped -
// an unbound GenericBox receiver cannot be constructed in C++.

// ===========================================================================
// SecC C4 - caller-specified binding contradicted by the actual value
// ===========================================================================

// test_make_triple_types_kwarg_contradicted_by_actual_rejects: skipped -
// passing a string where the seeded A=int formal expects int64_t is a C++
// compile error; the engine's Gate B rejection is unreachable.
// test_make_triple_full_subscript_contradicted_by_actual_rejects: skipped -
// same (compile error).

// ===========================================================================
// SecB/SecD - heterogeneous array unification
// ===========================================================================

BAML_TEST(elem_type_homogeneous_array_is_single_type) {
  // The degenerate case: a homogeneous array is a single type.
  BAML_ASSERT_EQ(generic_tests::elem_type(std::vector<int64_t>{1, 2, 3}),
                 std::string("int"));
}

// ===========================================================================
// SecG generalized - unbound instances recovered via the formal
// ===========================================================================

// test_read_items_unbound_container_recovers_T_from_fields: skipped - unbound
// generic instances cannot be constructed in C++.
// test_list_head_unbound_recursive_recovers_T_from_fields: skipped - same.
// test_extract_fully_unbound_nested_pair_recovers_all_vars: skipped - same.

// ===========================================================================
// SecD concrete-type join
// ===========================================================================

// test_triple_choose_join_includes_concrete_class: skipped - three
// differently-typed arguments are a C++ deduction conflict; the engine's
// covariant join is unreachable.
// test_triple_choose_join_includes_enum_variant: skipped - same.
