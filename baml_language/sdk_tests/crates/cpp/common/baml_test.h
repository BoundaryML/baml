#ifndef BAML_TEST_H_
#define BAML_TEST_H_

// Minimal, dependency-free test harness for the C++ sdk-test fixtures.
// Each customizable/tests/*.cc registers cases with BAML_TEST(name);
// exactly one file per fixture invokes BAML_TEST_MAIN(). The runner prints
// one line per case and exits non-zero on any failure, which is all
// test.sh / run_test_cmd need.

#include <cstdio>
#include <exception>
#include <string>
#include <utility>
#include <vector>

namespace baml_test {

struct Case {
  const char* name;
  void (*fn)();
};

inline std::vector<Case>& Registry() {
  static std::vector<Case> cases;
  return cases;
}

struct Register {
  Register(const char* name, void (*fn)()) {
    Registry().push_back(Case{name, fn});
  }
};

struct Failure {
  std::string message;
};

// Child-process entry points (for tests that must observe process exit
// codes, e.g. the baml.sys.exit contract). Registered with
// BAML_TEST_CHILD(name); the parent re-executes itself as
// `<binary> --child <name>`.
struct Child {
  const char* name;
  int (*fn)();
};

inline std::vector<Child>& ChildRegistry() {
  static std::vector<Child> children;
  return children;
}

struct RegisterChild {
  RegisterChild(const char* name, int (*fn)()) {
    ChildRegistry().push_back(Child{name, fn});
  }
};

inline const char*& Argv0Storage() {
  static const char* argv0 = "";
  return argv0;
}

// The test binary's own path, for spawning child processes.
inline const char* Argv0() { return Argv0Storage(); }

inline int RunChild(const char* name) {
  for (const Child& c : ChildRegistry()) {
    if (std::string(c.name) == name) {
      return c.fn();
    }
  }
  std::fprintf(stderr, "unknown --child '%s'\n", name);
  return 127;
}

[[noreturn]] inline void Fail(std::string message) {
  throw Failure{std::move(message)};
}

inline int RunAll() {
  int failed = 0;
  for (const Case& c : Registry()) {
    try {
      c.fn();
      std::printf("PASS %s\n", c.name);
    } catch (const Failure& f) {
      std::printf("FAIL %s: %s\n", c.name, f.message.c_str());
      ++failed;
    } catch (const std::exception& e) {
      std::printf("FAIL %s: unexpected exception: %s\n", c.name, e.what());
      ++failed;
    } catch (...) {
      std::printf("FAIL %s: unexpected non-std exception\n", c.name);
      ++failed;
    }
  }
  std::printf("%zu tests, %d failed\n", Registry().size(), failed);
  return failed == 0 ? 0 : 1;
}

}  // namespace baml_test

#define BAML_TEST(name)                                                      \
  static void baml_test_case_##name();                                       \
  static ::baml_test::Register baml_test_reg_##name{#name,                   \
                                                    &baml_test_case_##name}; \
  static void baml_test_case_##name()

#define BAML_STRINGIZE_INNER(x) #x
#define BAML_STRINGIZE(x) BAML_STRINGIZE_INNER(x)

#define BAML_ASSERT(cond)                                        \
  do {                                                           \
    if (!(cond)) {                                               \
      ::baml_test::Fail(std::string(__FILE__ ":" BAML_STRINGIZE( \
                            __LINE__) ": assertion failed: ") +  \
                        #cond);                                  \
    }                                                            \
  } while (0)

#define BAML_ASSERT_EQ(lhs, rhs)                                 \
  do {                                                           \
    if (!((lhs) == (rhs))) {                                     \
      ::baml_test::Fail(std::string(__FILE__ ":" BAML_STRINGIZE( \
                            __LINE__) ": assertion failed: ") +  \
                        #lhs " == " #rhs);                       \
    }                                                            \
  } while (0)

#define BAML_TEST_CHILD(name)                                   \
  static int baml_test_child_##name();                          \
  static ::baml_test::RegisterChild baml_test_child_reg_##name{ \
      #name, &baml_test_child_##name};                          \
  static int baml_test_child_##name()

#define BAML_TEST_MAIN()                                  \
  int main(int argc, char** argv) {                       \
    ::baml_test::Argv0Storage() = argv[0];                \
    if (argc >= 3 && std::string(argv[1]) == "--child") { \
      return ::baml_test::RunChild(argv[2]);              \
    }                                                     \
    (void)argc;                                           \
    return ::baml_test::RunAll();                         \
  }

#endif  // BAML_TEST_H_
