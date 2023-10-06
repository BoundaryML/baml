#pragma once
#include <filesystem>
#include <fstream>
#include <sstream>
#include <string>
#include <tuple>
#include <unordered_map>
#include <vector>

namespace gloo {
class FileWriter {
 public:
  FileWriter() {}

  virtual void flush(const std::filesystem::path& path) = 0;
  std::unique_ptr<std::stringstream> stream();

  void add_import(const std::string& import_path,
                  const std::string& import_name, bool export_imports = false) {
    // Check if the import is already added
    for (const auto& [path, name, _] : imports) {
      if (path == import_path && name == import_name) {
        return;
      }
    }
    imports.push_back({import_path, import_name, export_imports});
  }

  void add_template_var(const std::string& key, const std::string& value) {
    template_vars[key] = value;
  }

 private:
  // Create a stream which after closing will be added to the map of values.
  class BufferStream : public std::stringstream {
   public:
    BufferStream(FileWriter* writer) : writer_(writer) {}

    ~BufferStream() { flush(); }

    void flush() { writer_->content += str(); }

   private:
    FileWriter* writer_;
  };

 protected:
  std::unordered_map<std::string, std::string> template_vars;
  std::string content;
  std::vector<std::tuple<std::string, std::string, bool>> imports;
};

class PyFileWriter final : public FileWriter {
 public:
  void flush(const std::filesystem::path& path) override;
};

class DirectoryWriter {
 public:
  static DirectoryWriter& get() {
    static DirectoryWriter instance;
    return instance;
  }

  std::shared_ptr<FileWriter> file(const char* const path) {
    return file(std::filesystem::path(path));
  }
  std::shared_ptr<FileWriter> file(const std::filesystem::path& path) {
    std::string path_str = path.string();
    if (file_map.find(path_str) == file_map.end()) {
      file_map[path_str] = std::shared_ptr<PyFileWriter>(new PyFileWriter());
    }
    return file_map[path_str];
  }

  void flush(std::filesystem::path root_path) {
    const auto temp_path = root_path.parent_path() / std::string(".gloo.temp");
    // Ensure the path is a directory if it exists.
    std::filesystem::create_directories(temp_path);
    for (const auto& [path, writer] : file_map) {
      writer->flush(temp_path / path);
    }
    // Write a special py.typed file to indicate this is a python package.
    std::ofstream typed_file(temp_path / std::string("py.typed"));
    typed_file.close();

    // If the root path exists, delete it.
    if (std::filesystem::exists(root_path)) {
      std::filesystem::remove_all(root_path);
    }
    std::filesystem::rename(temp_path, root_path);
  }

 private:
  DirectoryWriter() {}
  DirectoryWriter(const DirectoryWriter&) = delete;
  void operator=(const DirectoryWriter&) = delete;

  std::unordered_map<std::string, std::shared_ptr<FileWriter>> file_map;
};

}  // namespace gloo