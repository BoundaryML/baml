#ifndef BAML_TEST_H_
#define BAML_TEST_H_

// Minimal, dependency-free test harness for the C++ sdk-test fixtures.
// Each customizable/tests/*.cc registers cases with BAML_TEST(name);
// exactly one file per fixture invokes BAML_TEST_MAIN(). The runner prints
// one line per case and exits non-zero on any failure, which is all
// test.sh / run_test_cmd need.

#include <cstdio>
#include <cstdlib>
#include <exception>
#include <string>
#include <utility>
#include <vector>

namespace baml_test {

struct test_case {
  const char* name;
  void (*fn)();
};

inline std::vector<test_case>& registry() {
  static std::vector<test_case> cases;
  return cases;
}

inline const char*& executable_path_storage() {
  static const char* path = nullptr;
  return path;
}

inline const char* executable_path() { return executable_path_storage(); }

struct registrar {
  registrar(const char* name, void (*fn)()) {
    registry().push_back(test_case{name, fn});
  }
};

struct failure {
  std::string message;
};

[[noreturn]] inline void fail(std::string message) {
  throw failure{std::move(message)};
}

inline int run_all() {
  int failed = 0;
  std::size_t ran = 0;
  const char* filter = std::getenv("BAML_TEST_FILTER");
  for (const test_case& c : registry()) {
    if (filter != nullptr && std::string(c.name) != filter) {
      continue;
    }
    ++ran;
    try {
      c.fn();
      std::printf("PASS %s\n", c.name);
    } catch (const failure& f) {
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
  if (filter != nullptr && ran == 0) {
    std::printf("FAIL no test matched %s\n", filter);
    ++failed;
  }
  std::printf("%zu tests, %d failed\n", ran, failed);
  return failed == 0 ? 0 : 1;
}

}  // namespace baml_test

#define BAML_TEST(name)                                                       \
  static void baml_test_case_##name();                                        \
  static ::baml_test::registrar baml_test_reg_##name{#name,                   \
                                                     &baml_test_case_##name}; \
  static void baml_test_case_##name()

#define BAML_STRINGIZE_INNER(x) #x
#define BAML_STRINGIZE(x) BAML_STRINGIZE_INNER(x)

#define BAML_ASSERT(cond)                                        \
  do {                                                           \
    if (!(cond)) {                                               \
      ::baml_test::fail(std::string(__FILE__ ":" BAML_STRINGIZE( \
                            __LINE__) ": assertion failed: ") +  \
                        #cond);                                  \
    }                                                            \
  } while (0)

#define BAML_ASSERT_EQ(lhs, rhs)                                 \
  do {                                                           \
    if (!((lhs) == (rhs))) {                                     \
      ::baml_test::fail(std::string(__FILE__ ":" BAML_STRINGIZE( \
                            __LINE__) ": assertion failed: ") +  \
                        #lhs " == " #rhs);                       \
    }                                                            \
  } while (0)

#define BAML_TEST_MAIN()                              \
  int main(int argc, char** argv) {                   \
    (void)argc;                                       \
    ::baml_test::executable_path_storage() = argv[0]; \
    return ::baml_test::run_all();                    \
  }

#endif  // BAML_TEST_H_
