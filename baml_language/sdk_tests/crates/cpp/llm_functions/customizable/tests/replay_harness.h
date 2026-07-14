#ifndef REPLAY_HARNESS_H_
#define REPLAY_HARNESS_H_

// Keyless replay harness shared by the streaming tests.
// Port of llm_functions/customizable/replay_harness.py: ReplayServer runs
// an in-process BAML server replaying a checked-in SSE recording on a
// background thread, with the env-driven StreamStub client pointed at it
// -- so a test exercises the full streaming path with no OPENAI_API_KEY.
// POSIX-only (setenv + raw-socket shutdown POST), like the cpp harness's
// other server-backed tests.
#include <arpa/inet.h>
#include <baml_sdk.h>
#include <netinet/in.h>
#include <sys/socket.h>
#include <unistd.h>

#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <sstream>
#include <stdexcept>
#include <string>
#include <thread>

namespace replay {

// Absolute-enough path to a checked-in SSE recording: the test binary runs
// from <fixture>/generated/, four levels below sdk_tests/.
inline std::string RecordingPath(const std::string& name) {
  const std::string rel =
      "../../../../fixtures/llm_functions/recordings/" + name + ".snap.sse";
  if (!std::ifstream(rel)) {
    throw std::runtime_error("missing recording " + rel);
  }
  return rel;
}

// POSTs an empty body to http://<host_port><path> (fire-and-forget).
inline void PostEmpty(const std::string& host_port, const std::string& path) {
  const size_t colon = host_port.find(':');
  if (colon == std::string::npos) {
    return;
  }
  const std::string host = host_port.substr(0, colon);
  const int port = std::atoi(host_port.c_str() + colon + 1);
  const int fd = ::socket(AF_INET, SOCK_STREAM, 0);
  if (fd < 0) {
    return;
  }
  sockaddr_in addr{};
  addr.sin_family = AF_INET;
  addr.sin_port = htons(static_cast<uint16_t>(port));
  ::inet_pton(AF_INET, host.c_str(), &addr.sin_addr);
  if (::connect(fd, reinterpret_cast<sockaddr*>(&addr), sizeof(addr)) == 0) {
    const std::string request = "POST " + path +
                                " HTTP/1.1\r\n"
                                "Host: " +
                                host_port +
                                "\r\n"
                                "Content-Length: 0\r\n"
                                "Connection: close\r\n"
                                "\r\n";
    (void)::send(fd, request.data(), request.size(), 0);
    char buf[512];
    (void)::recv(fd, buf, sizeof(buf), 0);
  }
  ::close(fd);
}

// Serves `recording` for the lifetime of this object: the BAML-implemented
// replay server runs on a background thread, BAML_REPLAY_BASE_URL /
// BAML_REPLAY_API_KEY point the StreamStub client at it, and destruction
// shuts the server down via its shutdown endpoint.
class ReplayServer {
 public:
  explicit ReplayServer(const std::string& recording) {
    const std::string rec = RecordingPath(recording);
    addr_file_ = "baml_replay_" + std::to_string(::getpid()) + "_" + recording;
    std::remove(addr_file_.c_str());

    thread_ = std::thread([rec, addr_file = addr_file_]() {
      try {
        baml_sdk::replay::replay_serve_until_shutdown(rec, addr_file);
      } catch (...) {
        // Surfaced by the poller below as a bind timeout.
      }
    });

    const auto deadline =
        std::chrono::steady_clock::now() + std::chrono::seconds(10);
    while (std::chrono::steady_clock::now() < deadline) {
      std::ifstream in(addr_file_);
      if (in) {
        std::ostringstream buf;
        buf << in.rdbuf();
        addr_ = buf.str();
        while (!addr_.empty() &&
               (addr_.back() == '\n' || addr_.back() == ' ')) {
          addr_.pop_back();
        }
        if (!addr_.empty()) {
          break;
        }
      }
      std::this_thread::sleep_for(std::chrono::milliseconds(20));
    }
    if (addr_.empty()) {
      thread_.detach();
      throw std::runtime_error("replay server did not bind within 10s");
    }
    ::setenv("BAML_REPLAY_BASE_URL", ("http://" + addr_).c_str(), 1);
    ::setenv("BAML_REPLAY_API_KEY", "replay-test-key", 1);
  }

  ~ReplayServer() {
    PostEmpty(addr_, "/__replay__/shutdown");
    if (thread_.joinable()) {
      thread_.join();
    }
    std::remove(addr_file_.c_str());
    ::unsetenv("BAML_REPLAY_BASE_URL");
    ::unsetenv("BAML_REPLAY_API_KEY");
  }

  ReplayServer(const ReplayServer&) = delete;
  ReplayServer& operator=(const ReplayServer&) = delete;

 private:
  std::string addr_file_;
  std::string addr_;
  std::thread thread_;
};

}  // namespace replay

#endif  // REPLAY_HARNESS_H_
