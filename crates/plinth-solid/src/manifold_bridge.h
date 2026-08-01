#pragma once

#include "rust/cxx.h"

#include <cstdint>

namespace plinth::solid {

struct FfiMesh;
struct FfiResponse;

FfiResponse manifold_union_batch(rust::Slice<const double> vertices,
                                 rust::Slice<const std::uint64_t> triangles,
                                 rust::Slice<const std::uint64_t> vertex_offsets,
                                 rust::Slice<const std::uint64_t> triangle_offsets) noexcept;

FfiResponse manifold_difference_batch(
    rust::Slice<const double> vertices,
    rust::Slice<const std::uint64_t> triangles,
    rust::Slice<const std::uint64_t> vertex_offsets,
    rust::Slice<const std::uint64_t> triangle_offsets) noexcept;

}  // namespace plinth::solid
