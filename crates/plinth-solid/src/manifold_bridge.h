#pragma once

#include "rust/cxx.h"

#include <cstdint>

namespace plinth::solid {

struct BackendConfig;
struct FfiMesh;

BackendConfig manifold_config() noexcept;

FfiMesh manifold_union_batch(rust::Slice<const double> vertices,
                             rust::Slice<const std::uint64_t> triangles,
                             rust::Slice<const std::uint64_t> vertex_offsets,
                             rust::Slice<const std::uint64_t> triangle_offsets);

FfiMesh manifold_difference_batch(
    rust::Slice<const double> vertices,
    rust::Slice<const std::uint64_t> triangles,
    rust::Slice<const std::uint64_t> vertex_offsets,
    rust::Slice<const std::uint64_t> triangle_offsets);

}  // namespace plinth::solid
