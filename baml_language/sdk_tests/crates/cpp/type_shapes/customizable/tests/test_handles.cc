// Coverage for handle-backed stdlib types returned from BAML to C++.
// Port of type_shapes/customizable/roundtrip_tests/test_handles.py.
//
// The non-media cases are encode-back tests: C++ receives a generated
// class instance with an embedded baml::Handle, calls generated stdlib
// methods with that same instance, and the engine must see the original
// handle state. No external dependency: the HTTP test binds an ephemeral
// localhost server and the FS test uses a temp file.
//
// Deviation from the Python file: the HTTP server test is POSIX-only (raw
// sockets stand in for Python's http.server).
#include <baml_sdk.h>
#include <baml_test.h>

#include <cstdio>
#include <optional>
#include <string>

#ifndef _WIN32
#include <netinet/in.h>
#include <sys/socket.h>
#include <unistd.h>

#include <atomic>
#include <cstring>
#include <thread>
#endif

namespace {

// Writes a small fixture file and removes it on scope exit.
class TempFile {
 public:
  explicit TempFile(const std::string& contents) {
    path_ = "handles_test_digits.txt";
    std::FILE* f = std::fopen(path_.c_str(), "w");
    BAML_ASSERT(f != nullptr);
    std::fwrite(contents.data(), 1, contents.size(), f);
    std::fclose(f);
  }
  ~TempFile() { std::remove(path_.c_str()); }
  const std::string& path() const { return path_; }

 private:
  std::string path_;
};

}  // namespace

// --- media: Image.from_base64 ----------------------------------------------

// 1x1 transparent PNG.
static const char kPngB64[] =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk"
    "+M8AAAQEAQB9eIv5AAAAAElFTkSuQmCC";

BAML_TEST(image_from_base64_roundtrips_payload) {
  const baml_sdk::baml::media::Image img =
      baml_sdk::baml::media::Image::from_base64(kPngB64,
                                                std::string("image/png"));
  BAML_ASSERT(img.mime_type() == std::optional<std::string>("image/png"));
  BAML_ASSERT_EQ(img.base64(), std::string(kPngB64));
}

// --- baml.fs.File: cursor state preserved across calls --------------------

BAML_TEST(open_file_returns_file_handle) {
  const TempFile tmp("0123456789");
  const baml_sdk::baml::fs::File f = baml_sdk::baml::fs::open(tmp.path(), "r");
  BAML_ASSERT(!f._handle.empty());
  f.close();
}

BAML_TEST(file_cursor_state_persists_across_calls) {
  const TempFile tmp("0123456789");
  const baml_sdk::baml::fs::File f = baml_sdk::baml::fs::open(tmp.path(), "r");

  // Two successive reads on the *same* handle must advance the cursor --
  // the second read continues where the first stopped. This is the
  // load-bearing assertion: engine-side file state survives across
  // separate host->engine FFI calls.
  BAML_ASSERT_EQ(f.read(3), std::string("012"));
  BAML_ASSERT_EQ(f.read(3), std::string("345"));

  // Seek back to the start and confirm the cursor actually moved.
  BAML_ASSERT_EQ(f.seek_from("start", 0), int64_t{0});
  BAML_ASSERT_EQ(f.read(2), std::string("01"));

  // text() reads from the current cursor (now at 2) to EOF.
  BAML_ASSERT_EQ(f.text(), std::string("23456789"));

  f.close();
}

// --- baml.http.Response ----------------------------------------------------

#ifndef _WIN32

namespace {

const char kHttpBody[] = "hello from localhost";

// Minimal one-shot HTTP server: binds an ephemeral localhost port, serves
// exactly one GET with a fixed 200 response on a background thread.
class OneShotHttpServer {
 public:
  OneShotHttpServer() {
    fd_ = ::socket(AF_INET, SOCK_STREAM, 0);
    BAML_ASSERT(fd_ >= 0);
    sockaddr_in addr{};
    addr.sin_family = AF_INET;
    addr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    addr.sin_port = 0;
    BAML_ASSERT(::bind(fd_, reinterpret_cast<sockaddr*>(&addr), sizeof(addr)) ==
                0);
    socklen_t len = sizeof(addr);
    BAML_ASSERT(::getsockname(fd_, reinterpret_cast<sockaddr*>(&addr), &len) ==
                0);
    port_ = ntohs(addr.sin_port);
    BAML_ASSERT(::listen(fd_, 1) == 0);
    thread_ = std::thread([this]() { ServeOne(); });
  }

  ~OneShotHttpServer() {
    thread_.join();
    ::close(fd_);
  }

  std::string url() const {
    return "http://127.0.0.1:" + std::to_string(port_) + "/";
  }

 private:
  void ServeOne() {
    const int conn = ::accept(fd_, nullptr, nullptr);
    if (conn < 0) {
      return;
    }
    char buf[4096];
    // Read until the end of the request headers (single small GET).
    std::string request;
    while (request.find("\r\n\r\n") == std::string::npos) {
      const ssize_t n = ::recv(conn, buf, sizeof(buf), 0);
      if (n <= 0) {
        break;
      }
      request.append(buf, static_cast<size_t>(n));
    }
    const std::string body = kHttpBody;
    const std::string response =
        "HTTP/1.1 200 OK\r\n"
        "Content-Type: text/plain\r\n"
        "Content-Length: " +
        std::to_string(body.size()) +
        "\r\n"
        "Connection: close\r\n"
        "\r\n" +
        body;
    (void)::send(conn, response.data(), response.size(), 0);
    ::close(conn);
  }

  int fd_ = -1;
  uint16_t port_ = 0;
  std::thread thread_;
};

}  // namespace

BAML_TEST(http_get_response_fields_and_methods) {
  OneShotHttpServer server;
  const baml_sdk::baml::http::Response resp =
      baml_sdk::baml::http::fetch(server.url());
  BAML_ASSERT_EQ(resp.status_code, int64_t{200});
  BAML_ASSERT(resp.ok() == true);
  BAML_ASSERT_EQ(resp.text(), std::string(kHttpBody));
}

#endif  // !_WIN32
