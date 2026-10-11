// The in-memory filesystem (memfs.cc) as plain C functions, for the Rust half
// of the core to call.
// SPDX-License-Identifier: MIT
#include "memfs.h"

#include <cstring>

namespace {
int64_t g_now = 0;
int64_t now() { return g_now; }
std::vector<std::string> g_list;
std::vector<uint8_t> g_file;
}  // namespace

extern "C" {

// Every file's time is this one, in seconds since 1970: the date the console
// was switched on.
void chimera_memfs_set_time(int64_t seconds) {
  g_now = seconds;
  chimera::memfs::set_clock(now);
}

int chimera_memfs_mkdirs(const char* path) { return chimera::memfs::mkdirs(path) ? 1 : 0; }

// A read-only file in the tree that is read from outside it: a path on the
// host natively, a mounted file's name in the sandbox.
int chimera_memfs_graft(const char* path, const char* host_name) {
  return chimera::memfs::graft(path, host_name) ? 1 : 0;
}

// A writable file with these contents, its folders made as needed.
int chimera_memfs_put(const char* path, const uint8_t* data, size_t size) {
  const std::string p(path);
  const size_t slash = p.rfind('/');
  if (slash != std::string::npos && slash > 0 && !chimera::memfs::mkdirs(p.substr(0, slash))) {
    return 0;
  }
  return chimera::memfs::put(p, std::vector<uint8_t>(data, data + size)) ? 1 : 0;
}

// Every regular file under a folder, in name order. The count; then each
// name by index until the next call.
int chimera_memfs_list(const char* dir) {
  g_list = chimera::memfs::list(dir);
  return static_cast<int>(g_list.size());
}
const char* chimera_memfs_list_name(int index) {
  return index >= 0 && static_cast<size_t>(index) < g_list.size() ? g_list[index].c_str() : "";
}

// What a writable file holds now. Its size, or -1; then the bytes until the
// next call.
int64_t chimera_memfs_get(const char* path) {
  g_file.clear();
  return chimera::memfs::get(path, g_file) ? static_cast<int64_t>(g_file.size()) : -1;
}
const uint8_t* chimera_memfs_get_bytes(void) { return g_file.data(); }

}  // extern "C"
