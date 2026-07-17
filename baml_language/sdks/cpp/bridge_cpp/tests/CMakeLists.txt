# Bridge-core smoke build: the header-only bridge + checked-in pb sources
# over the pinned protobuf-lite runtime. Driven by tests/run.sh.
cmake_minimum_required(VERSION 3.16)
project(bridge_cpp_tests LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

include(${CMAKE_CURRENT_SOURCE_DIR}/../cmake/fetch_protobuf.cmake)

set(PB_DIR ${CMAKE_CURRENT_SOURCE_DIR}/../pb/baml_bridge/cffi/v1)
add_executable(runtime_smoke
  runtime_smoke.cc
  ${PB_DIR}/baml_handle.pb.cc
  ${PB_DIR}/baml_inbound.pb.cc
  ${PB_DIR}/baml_outbound.pb.cc
  ${PB_DIR}/baml_type.pb.cc)
target_include_directories(runtime_smoke PRIVATE
  ${CMAKE_CURRENT_SOURCE_DIR}/../include
  ${CMAKE_CURRENT_SOURCE_DIR}/../pb
  ${CMAKE_CURRENT_SOURCE_DIR}/../../../../crates/bridge_cffi/include)
find_package(Threads REQUIRED)
target_link_libraries(runtime_smoke PRIVATE
  protobuf::libprotobuf-lite Threads::Threads ${CMAKE_DL_LIBS})
if(NOT MSVC)
  target_compile_options(runtime_smoke PRIVATE -Wall -Wextra -Werror)
endif()
