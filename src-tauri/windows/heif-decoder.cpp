// Decode the HEIF primary image in a disposable process. libheif owns grid
// assembly, cropping, rotation, mirroring, color conversion and alpha decoding.
#include <windows.h>
#include <libheif/heif.h>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <vector>

constexpr uint64_t MAX_BYTES = 256ull * 1024 * 1024;
constexpr uint64_t MAX_PIXELS = 64ull * 1024 * 1024;

static void check(heif_error error) {
  if (error.code != heif_error_Ok) {
    throw std::runtime_error(error.message ? error.message : "HEIF decoding failed");
  }
}

class Output {
  HANDLE handle;
  std::wstring path;
  bool completed = false;
public:
  explicit Output(const wchar_t* name) : path(name) {
    handle = CreateFileW(name, GENERIC_WRITE, 0, nullptr, CREATE_NEW,
                         FILE_ATTRIBUTE_NORMAL, nullptr);
    if (handle == INVALID_HANDLE_VALUE) throw std::runtime_error("Cannot create HEIF output");
  }
  void write(const void* data, DWORD size) {
    DWORD written = 0;
    if (!WriteFile(handle, data, size, &written, nullptr) || written != size)
      throw std::runtime_error("Cannot write HEIF output");
  }
  void finish() { completed = true; }
  ~Output() {
    CloseHandle(handle);
    if (!completed) DeleteFileW(path.c_str());
  }
};

int wmain(int argc, wchar_t** argv) {
  if (argc == 2 && std::wstring(argv[1]) == L"--version") {
    std::cout << "ClipSeek HEIF decoder / libheif " << heif_get_version() << '\n';
    return 0;
  }
  if (argc != 3) { std::cerr << "Expected input and output paths\n"; return 2; }
  try {
    std::ifstream input(std::filesystem::path(argv[1]), std::ios::binary | std::ios::ate);
    auto size = input.tellg();
    if (size <= 0 || static_cast<uint64_t>(size) > MAX_BYTES)
      throw std::runtime_error("HEIF source is empty, inaccessible or exceeds the size limit");
    std::vector<uint8_t> data(static_cast<size_t>(size));
    input.seekg(0);
    if (!input.read(reinterpret_cast<char*>(data.data()), size))
      throw std::runtime_error("Cannot read HEIF source");

    std::unique_ptr<heif_context, decltype(&heif_context_free)> ctx(heif_context_alloc(), heif_context_free);
    if (!ctx) throw std::runtime_error("Cannot allocate HEIF context");
    auto* limits = heif_context_get_security_limits(ctx.get());
    limits->max_image_size_pixels = MAX_PIXELS;
    limits->max_memory_block_size = MAX_BYTES;
    limits->max_total_memory = 2 * MAX_BYTES;
    heif_context_set_max_decoding_threads(ctx.get(), 2);
    check(heif_context_read_from_memory_without_copy(ctx.get(), data.data(), data.size(), nullptr));
    heif_image_handle* raw_handle = nullptr;
    check(heif_context_get_primary_image_handle(ctx.get(), &raw_handle));
    std::unique_ptr<heif_image_handle, decltype(&heif_image_handle_release)> handle(raw_handle, heif_image_handle_release);
    const int width_hint = heif_image_handle_get_width(handle.get());
    const int height_hint = heif_image_handle_get_height(handle.get());
    if (width_hint <= 0 || height_hint <= 0 || uint64_t(width_hint) * height_hint > MAX_PIXELS)
      throw std::runtime_error("HEIF dimensions exceed the limit");
    std::unique_ptr<heif_decoding_options, decltype(&heif_decoding_options_free)> options(
        heif_decoding_options_alloc(), heif_decoding_options_free);
    if (!options) throw std::runtime_error("Cannot allocate HEIF options");
    options->strict_decoding = 1;
    options->convert_hdr_to_8bit = 1;
    options->num_codec_threads = 2;
    heif_image* raw_image = nullptr;
    check(heif_decode_image(handle.get(), &raw_image, heif_colorspace_RGB,
                           heif_chroma_interleaved_RGBA, options.get()));
    std::unique_ptr<heif_image, decltype(&heif_image_release)> image(raw_image, heif_image_release);
    if (heif_image_get_decoding_warnings(image.get(), 0, nullptr, 0) > 0)
      throw std::runtime_error("HEIF image contains decoding errors");
    // Read the actual decoded plane, not the signalled handle dimensions.
    const int width = heif_image_get_width(image.get(), heif_channel_interleaved);
    const int height = heif_image_get_height(image.get(), heif_channel_interleaved);
    if (width <= 0 || height <= 0 || uint64_t(width) * height > MAX_PIXELS)
      throw std::runtime_error("Decoded HEIF dimensions exceed the limit");
    size_t stride = 0;
    const uint8_t* pixels = heif_image_get_plane_readonly2(image.get(), heif_channel_interleaved, &stride);
    const auto row_size = uint64_t(width) * 4;
    if (!pixels || stride < row_size) throw std::runtime_error("Invalid HEIF pixel plane");
    Output output(argv[2]);
    const uint32_t dimensions[] = {uint32_t(width), uint32_t(height)};
    output.write("CSHF", 4);
    output.write(dimensions, sizeof(dimensions)); // Windows x64 little-endian
    for (int row = 0; row < height; ++row)
      output.write(pixels + size_t(row) * stride, DWORD(row_size));
    output.finish();
    return 0;
  } catch (const std::exception& error) {
    std::cerr << error.what() << '\n';
    return 1;
  }
}
