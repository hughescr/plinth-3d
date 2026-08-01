#include "plinth-solid/src/ffi.rs.h"

#include "manifold/manifold.h"

#include <cstddef>
#include <cstdint>
#include <limits>
#include <stdexcept>
#include <string>
#include <type_traits>
#include <utility>
#include <vector>

#if __cplusplus < 201703L
#error "plinth-solid requires C++17 or newer"
#endif

#ifndef MANIFOLD_PAR
#error "MANIFOLD_PAR must be set explicitly"
#endif

#if MANIFOLD_PAR != -1
#error "M0 plinth-solid must use the dependency-free serial Manifold backend"
#endif

#ifdef MANIFOLD_CROSS_SECTION
#error "M0 plinth-solid must not compile the cross-section backend"
#endif

static_assert(sizeof(double) == 8, "Manifold scalar must be IEEE-754 f64-sized");
static_assert(sizeof(std::uint64_t) == 8, "Manifold mesh index must be u64-sized");
static_assert(
    std::is_same_v<decltype(manifold::MeshGL64{}.triVerts)::value_type,
                   std::uint64_t>,
    "MeshGL64 must retain its u64 index ABI");

namespace plinth::solid {
namespace {

const char* status_name(manifold::Manifold::Error status) noexcept {
  using Error = manifold::Manifold::Error;
  switch (status) {
    case Error::NoError:
      return "NoError";
    case Error::NonFiniteVertex:
      return "NonFiniteVertex";
    case Error::NotManifold:
      return "NotManifold";
    case Error::VertexOutOfBounds:
      return "VertexOutOfBounds";
    case Error::PropertiesWrongLength:
      return "PropertiesWrongLength";
    case Error::MissingPositionProperties:
      return "MissingPositionProperties";
    case Error::MergeVectorsDifferentLengths:
      return "MergeVectorsDifferentLengths";
    case Error::MergeIndexOutOfBounds:
      return "MergeIndexOutOfBounds";
    case Error::TransformWrongLength:
      return "TransformWrongLength";
    case Error::RunIndexWrongLength:
      return "RunIndexWrongLength";
    case Error::FaceIDWrongLength:
      return "FaceIDWrongLength";
    case Error::InvalidConstruction:
      return "InvalidConstruction";
    case Error::ResultTooLarge:
      return "ResultTooLarge";
    case Error::InvalidTangents:
      return "InvalidTangents";
    case Error::Cancelled:
      return "Cancelled";
  }
  return "UnknownStatus";
}

void validate_offsets(rust::Slice<const double> vertices,
                      rust::Slice<const std::uint64_t> triangles,
                      rust::Slice<const std::uint64_t> vertex_offsets,
                      rust::Slice<const std::uint64_t> triangle_offsets) {
  if (vertex_offsets.size() != triangle_offsets.size() ||
      vertex_offsets.size() < 2) {
    throw std::invalid_argument(
        "mesh offset arrays must have equal length and at least two entries");
  }
  if (vertex_offsets[0] != 0 || triangle_offsets[0] != 0 ||
      vertex_offsets[vertex_offsets.size() - 1] != vertices.size() ||
      triangle_offsets[triangle_offsets.size() - 1] != triangles.size()) {
    throw std::invalid_argument("mesh offsets do not span the flattened buffers");
  }
  for (std::size_t i = 1; i < vertex_offsets.size(); ++i) {
    if (vertex_offsets[i] <= vertex_offsets[i - 1] ||
        triangle_offsets[i] <= triangle_offsets[i - 1] ||
        vertex_offsets[i] % 3 != 0 || triangle_offsets[i] % 3 != 0) {
      throw std::invalid_argument(
          "mesh offsets must be strictly increasing triangle-aligned triples");
    }
  }
}

std::vector<manifold::Manifold> import_batch(
    rust::Slice<const double> vertices,
    rust::Slice<const std::uint64_t> triangles,
    rust::Slice<const std::uint64_t> vertex_offsets,
    rust::Slice<const std::uint64_t> triangle_offsets) {
  validate_offsets(vertices, triangles, vertex_offsets, triangle_offsets);

  std::vector<manifold::Manifold> solids;
  solids.reserve(vertex_offsets.size() - 1);
  for (std::size_t mesh_index = 0; mesh_index + 1 < vertex_offsets.size();
       ++mesh_index) {
    manifold::MeshGL64 mesh;
    mesh.numProp = 3;
    mesh.vertProperties.assign(vertices.begin() + vertex_offsets[mesh_index],
                               vertices.begin() + vertex_offsets[mesh_index + 1]);
    mesh.triVerts.assign(triangles.begin() + triangle_offsets[mesh_index],
                         triangles.begin() + triangle_offsets[mesh_index + 1]);

    manifold::Manifold solid(mesh);
    const auto status = solid.Status();
    if (status != manifold::Manifold::Error::NoError) {
      throw std::runtime_error("Manifold rejected input mesh " +
                               std::to_string(mesh_index) + ": " +
                               status_name(status));
    }
    solids.push_back(std::move(solid));
  }
  return solids;
}

FfiMesh export_result(manifold::Manifold result) {
  const auto status = result.Status();
  if (status != manifold::Manifold::Error::NoError) {
    throw std::runtime_error(std::string("Manifold boolean failed: ") +
                             status_name(status));
  }

  const manifold::MeshGL64 mesh = result.GetMeshGL64();
  if (mesh.numProp < 3) {
    throw std::runtime_error("Manifold output has fewer than three coordinates");
  }

  FfiMesh out;
  const std::size_t num_vertices = mesh.NumVert();
  if (num_vertices > std::numeric_limits<std::size_t>::max() / 3) {
    throw std::overflow_error("Manifold output vertex count overflow");
  }
  out.vertices.reserve(num_vertices * 3);
  for (std::size_t vertex = 0; vertex < num_vertices; ++vertex) {
    const std::size_t base = vertex * mesh.numProp;
    out.vertices.push_back(mesh.vertProperties[base]);
    out.vertices.push_back(mesh.vertProperties[base + 1]);
    out.vertices.push_back(mesh.vertProperties[base + 2]);
  }
  out.triangles.reserve(mesh.triVerts.size());
  for (const std::uint64_t index : mesh.triVerts) {
    out.triangles.push_back(index);
  }
  return out;
}

}  // namespace

BackendConfig manifold_config() noexcept {
  return BackendConfig{
      3,
      5,
      2,
      MANIFOLD_PAR,
      static_cast<std::uint32_t>(sizeof(double)),
      static_cast<std::uint32_t>(sizeof(std::uint64_t)),
      false,
  };
}

FfiMesh manifold_union_batch(rust::Slice<const double> vertices,
                             rust::Slice<const std::uint64_t> triangles,
                             rust::Slice<const std::uint64_t> vertex_offsets,
                             rust::Slice<const std::uint64_t> triangle_offsets) {
  auto solids = import_batch(vertices, triangles, vertex_offsets, triangle_offsets);
  return export_result(manifold::Manifold::BatchBoolean(solids, manifold::OpType::Add));
}

FfiMesh manifold_difference_batch(
    rust::Slice<const double> vertices,
    rust::Slice<const std::uint64_t> triangles,
    rust::Slice<const std::uint64_t> vertex_offsets,
    rust::Slice<const std::uint64_t> triangle_offsets) {
  auto solids = import_batch(vertices, triangles, vertex_offsets, triangle_offsets);
  if (solids.size() < 2) {
    throw std::invalid_argument(
        "difference requires a minuend and at least one subtrahend");
  }
  std::vector<manifold::Manifold> tail;
  tail.reserve(solids.size() - 1);
  for (std::size_t i = 1; i < solids.size(); ++i) {
    tail.push_back(std::move(solids[i]));
  }
  auto subtractor = manifold::Manifold::BatchBoolean(tail, manifold::OpType::Add);
  return export_result(solids.front().Boolean(subtractor, manifold::OpType::Subtract));
}

}  // namespace plinth::solid
