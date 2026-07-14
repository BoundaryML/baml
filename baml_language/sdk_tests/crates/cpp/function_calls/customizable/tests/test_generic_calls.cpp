// Generic function-call coverage (ns_generic_tests), explicit-binding form.
// Port of function_calls/customizable/test_generic_calls.py. Deviations:
// - Python's `fn[T](...)` subscript becomes explicit C++ template args
//   (`fn<int64_t>(...)`); both surfaces send the same named type-arg wire.
// - Generic statics need a parameterized class to be named in C++
//   (`GenericBox<int64_t>::new_(...)`), so the generated binding also sends
//   the class-level TypeVar (`T`) alongside the static's own (`V`); Python
//   sends only `V`. Same for NamedStatic<A,B,C>::make (A/B/C ride along).
//   `new` is a C++ keyword, so the emitter sanitizes it to `new_`.
// - Negative runtime cases are compile errors in C++ and are skipped:
//   test_generic_free_fn_requires_binding (a bare call to a template with no
//   deducible params does not compile), test_subscript_wrong_arity_raises
//   (template arity is checked at compile time), and
//   test_instance_method_unparameterized_receiver_raises (an unparameterized
//   GenericBox cannot be constructed - the template requires a type arg).
// - The reified `_type_args(obj)` assertions have no runtime counterpart:
//   the concrete type args ARE the function's static C++ return type.
#include <baml_sdk.hpp>
#include <baml_test.hpp>

namespace generic_tests = baml_sdk::generic_tests;
using generic_tests::ContainerShapes;
using generic_tests::GenericBox;
using generic_tests::GenericPair;
using generic_tests::GenericRecursive;
using generic_tests::GenericTriple;
using generic_tests::NamedStatic;
using generic_tests::StringIntPair;

// ===========================================================================
// basic cases, free functions
// ===========================================================================

BAML_TEST(identity_explicit) {
    BAML_ASSERT_EQ(generic_tests::identity<int64_t>(5), int64_t{5});
    BAML_ASSERT_EQ(generic_tests::identity<std::string>("hi"), std::string("hi"));

    const StringIntPair pair{"a", 1};
    BAML_ASSERT(generic_tests::identity<StringIntPair>(pair) == pair);

    const GenericBox<GenericBox<std::string>> box{GenericBox<std::string>{"hello"}};
    BAML_ASSERT((generic_tests::identity<GenericBox<GenericBox<std::string>>>(box)) == box);

    const GenericTriple<GenericBox<std::string>, double, bool> triple{
        GenericBox<std::string>{"hello"},
        {1.1, 2.2},
        {{"lorem", true}, {"ipsum", false}},
    };
    BAML_ASSERT(
        (generic_tests::identity<GenericTriple<GenericBox<std::string>, double, bool>>(triple)) ==
        triple);
}

BAML_TEST(identity_async_explicit) {
    BAML_ASSERT_EQ(generic_tests::identity_async<int64_t>(7).get(), int64_t{7});
}

BAML_TEST(tag_or_value_explicit) {
    // `tag_or_value` reflects its bound `T` back as a string; `x` must inhabit
    // the substituted `T | string | null`.
    BAML_ASSERT_EQ(generic_tests::tag_or_value<int64_t>(std::variant<int64_t, std::string>{
                       int64_t{5}}),
                   std::string("int"));
    // T=string makes the parameter variant<string, string>; the duplicate
    // alternative needs in_place_index (the engine dedups the union).
    BAML_ASSERT_EQ(generic_tests::tag_or_value<std::string>(std::variant<std::string, std::string>{
                       std::in_place_index<0>, "plain"}),
                   std::string("string"));
    const StringIntPair pair{"b", 2};
    const std::string tagged =
        generic_tests::tag_or_value<StringIntPair>(std::variant<StringIntPair, std::string>{pair});
    BAML_ASSERT(tagged.find("StringIntPair") != std::string::npos);
}

BAML_TEST(make_triple_explicit) {
    // A=int, B=string, C=bool, bound positionally by the template args.
    const GenericTriple<int64_t, std::string, bool> t =
        generic_tests::make_triple<int64_t, std::string, bool>(1, {"a", "b"}, {{"k", true}});
    BAML_ASSERT_EQ(t.first, int64_t{1});
    BAML_ASSERT(t.second == (std::vector<std::string>{"a", "b"}));
    BAML_ASSERT(t.third == (std::map<std::string, bool>{{"k", true}}));
}

// one_type_arg<T>() / two_type_args<A,B>(): return-position-only TypeVars.
// No argument carries `T`; the binding can only come from the template args.

BAML_TEST(one_type_arg_explicit) {
    BAML_ASSERT_EQ(generic_tests::one_type_arg<int64_t>(), std::string("int"));
    BAML_ASSERT_EQ(generic_tests::one_type_arg<std::string>(), std::string("string"));
    // Nested generic binding must encode fully (base class + concrete arg).
    const std::string nested = generic_tests::one_type_arg<GenericBox<int64_t>>();
    BAML_ASSERT(nested.find("GenericBox") != std::string::npos);
    BAML_ASSERT(nested.find("int") != std::string::npos);
}

BAML_TEST(two_type_args_explicit) {
    BAML_ASSERT_EQ((generic_tests::two_type_args<int64_t, std::string>()),
                   std::string("int | string"));
}

// test_generic_free_fn_requires_binding: skipped - a bare `one_type_arg()` /
// `two_type_args()` does not compile in C++ (no deducible template params).
// test_subscript_wrong_arity_raises: skipped - `two_type_args<int64_t>()` is a
// compile-time arity error in C++.

// ===========================================================================
// basic cases, generic classes
// ===========================================================================

BAML_TEST(consume_int_wrapper_baseline) {
    // No binding of any kind: a concretely-instantiated GenericBox<int64_t>
    // flows in and the int field flows back out.
    BAML_ASSERT_EQ(generic_tests::consume_int_wrapper(GenericBox<int64_t>{9}), int64_t{9});
}

BAML_TEST(genericbox_get_explicit) {
    // GenericBox<int64_t> carries the type arg; the binding encodes it as the
    // method frame's class-level T.
    const GenericBox<int64_t> b{5};
    BAML_ASSERT_EQ(b.get(), std::string("int"));
}

BAML_TEST(genericbox_pair_with_explicit) {
    // T from the GenericBox<int64_t> receiver, U from the method template arg.
    const GenericBox<int64_t> b{5};
    BAML_ASSERT_EQ(b.pair_with<std::string>("hello world"), std::string("int | string"));
}

BAML_TEST(genericbox_new_static_explicit) {
    const GenericBox<int64_t> box = GenericBox<int64_t>::new_<int64_t>(5);
    BAML_ASSERT_EQ(box.value, int64_t{5});
}

BAML_TEST(generic_static_infers_binding) {
    // The static's own V appears in a parameter (value: V), so C++ deduces it
    // with no explicit method template arg (deduction still sends the binding
    // explicitly on the wire - see the file header).
    const GenericBox<int64_t> box = GenericBox<int64_t>::new_(int64_t{5});
    BAML_ASSERT_EQ(box.value, int64_t{5});
}

BAML_TEST(named_static_distinct_typevar_names) {
    // Static TypeVar names (D, E) differ from the class's (A, B, C). C++ must
    // parameterize the class to name the static, so arbitrary A/B/C bindings
    // ride along (Python sends none); the named wire slots D/E by name.
    BAML_ASSERT_EQ((NamedStatic<int64_t, int64_t, int64_t>::make<int64_t, std::string>(1, "x")),
                   std::string("int | string"));
}

// test_instance_method_unparameterized_receiver_raises: skipped - C++ cannot
// construct an unparameterized GenericBox (the class template requires a type
// argument), so the negative case is a compile error.

BAML_TEST(extract_explicit) {
    // Nested generic: extract<A,B,C,D>(a: GenericPair<GenericPair<A,B>,
    // GenericPair<C,D>>).
    const GenericPair<GenericPair<int64_t, std::string>, GenericPair<bool, double>> pair{
        GenericPair<int64_t, std::string>{1, "a"},
        GenericPair<bool, double>{true, 1.5},
    };
    BAML_ASSERT_EQ((generic_tests::extract<int64_t, std::string, bool, double>(pair)),
                   std::string("int | string | bool | float"));
}

// ===========================================================================
// basic case, T in return position only
// ===========================================================================

BAML_TEST(parse_as_explicit) {
    // T bound by the host via the template arg.
    const StringIntPair pair =
        generic_tests::parse_as<StringIntPair>("{\"my_string\": \"x\", \"my_int\": 3}");
    BAML_ASSERT(pair == (StringIntPair{"x", 3}));
    BAML_ASSERT_EQ(generic_tests::parse_as<int64_t>("42"), int64_t{42});
}

// ===========================================================================
// complex cases
// ===========================================================================

BAML_TEST(second_of_explicit) {
    BAML_ASSERT_EQ(generic_tests::second_of<std::string>(
                       GenericPair<int64_t, std::string>{1, "hi"}),
                   std::string("hi"));
    const StringIntPair pair{"z", 9};
    const GenericPair<int64_t, StringIntPair> p{0, pair};
    BAML_ASSERT(generic_tests::second_of<StringIntPair>(p) == pair);
}

BAML_TEST(list_head_explicit) {
    const GenericRecursive<int64_t> linked_list{
        7, GenericRecursive<int64_t>{8, std::nullopt}};
    BAML_ASSERT_EQ(generic_tests::list_head<int64_t>(linked_list), int64_t{7});
}

BAML_TEST(choose_explicit) {
    BAML_ASSERT_EQ(generic_tests::choose<int64_t>(1, 2), int64_t{1});
    BAML_ASSERT_EQ(generic_tests::choose<std::string>("a", "b"), std::string("a"));
}

BAML_TEST(read_items_explicit) {
    const ContainerShapes<int64_t> container{
        1,
        {1, 2, 3},
        {{"k", 4}},
        std::nullopt,
        std::nullopt,
    };
    BAML_ASSERT(generic_tests::read_items<int64_t>(container) == (std::vector<int64_t>{1, 2, 3}));
}

// ===========================================================================
// outbound generics
// ===========================================================================

BAML_TEST(wrap_explicit) {
    const GenericBox<int64_t> w = generic_tests::wrap<int64_t>(5);
    BAML_ASSERT_EQ(w.value, int64_t{5});
}

// ===========================================================================
// reified generics returned by NON-generic functions
// ===========================================================================
// The outbound mirror of consume_int_wrapper: the callee's return type pins
// the class type args at the definition site. In C++ the reified type args
// are the static return type itself, so Python's __pydantic_generic_metadata__
// assertions are the (compile-checked) declarations below.

BAML_TEST(make_int_box_reified) {
    const GenericBox<int64_t> box = generic_tests::make_int_box();
    BAML_ASSERT_EQ(box.value, int64_t{7});
}

BAML_TEST(make_int_container_reified) {
    // The single int binding is reified into every field shape (bare, list,
    // map, optional, union).
    const ContainerShapes<int64_t> c = generic_tests::make_int_container();
    BAML_ASSERT_EQ(c.item, int64_t{1});
    BAML_ASSERT(c.items == (std::vector<int64_t>{1, 2, 3}));
    BAML_ASSERT(c.by_key == (std::map<std::string, int64_t>{{"k", 4}}));
    BAML_ASSERT(!c.maybe.has_value());
    BAML_ASSERT(c.mixed == (std::optional<std::variant<int64_t, std::string>>{
                    std::variant<int64_t, std::string>{int64_t{5}}}));
}

BAML_TEST(make_nested_box_reified) {
    // The type arg is itself a generic instance; the inner GenericBox decodes
    // out of the outer one's field.
    const GenericBox<GenericBox<int64_t>> outer = generic_tests::make_nested_box();
    BAML_ASSERT_EQ(outer.value.value, int64_t{9});
}

BAML_TEST(make_int_str_bool_triple_reified) {
    // Multiple TypeVars reified across mixed field shapes.
    const GenericTriple<int64_t, std::string, bool> t =
        generic_tests::make_int_str_bool_triple();
    BAML_ASSERT_EQ(t.first, int64_t{1});
    BAML_ASSERT(t.second == (std::vector<std::string>{"a", "b"}));
    BAML_ASSERT(t.third == (std::map<std::string, bool>{{"k", true}}));
}
