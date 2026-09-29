#include "plinth-solid/src/ffi.rs.h"

#include "manifold/manifold.h"

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <new>
#include <stdexcept>
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

using Mesh64PropertyContainer = std::remove_cv_t<std::remove_reference_t<
    decltype(manifold::MeshGL64{}.vertProperties)>>;

static_assert(sizeof(double) == 8, "Manifold scalar must be f64-sized");
static_assert(std::numeric_limits<double>::is_iec559,
              "Manifold scalar must implement IEEE-754");
static_assert(std::numeric_limits<double>::radix == 2,
              "Manifold scalar must use a binary radix");
static_assert(std::numeric_limits<double>::digits == 53,
              "Manifold scalar must be binary64 precision");
static_assert(std::numeric_limits<double>::max_exponent == 1024,
              "Manifold scalar must be binary64 exponent range");
static_assert(sizeof(std::uint64_t) == 8, "Manifold mesh index must be u64-sized");
static_assert(
    std::is_same_v<typename Mesh64PropertyContainer::value_type, double>,
    "MeshGL64 vertex properties must remain exactly double");
static_assert(
    std::is_same_v<decltype(manifold::MeshGL64{}.triVerts)::value_type,
                   std::uint64_t>,
    "MeshGL64 must retain its u64 index ABI");
static_assert(static_cast<int>(manifold::Manifold::Error::NoError) == 0);
static_assert(static_cast<int>(manifold::Manifold::Error::NonFiniteVertex) == 1);
static_assert(static_cast<int>(manifold::Manifold::Error::NotManifold) == 2);
static_assert(static_cast<int>(manifold::Manifold::Error::VertexOutOfBounds) == 3);
static_assert(static_cast<int>(manifold::Manifold::Error::PropertiesWrongLength) == 4);
static_assert(static_cast<int>(manifold::Manifold::Error::MissingPositionProperties) == 5);
static_assert(static_cast<int>(manifold::Manifold::Error::MergeVectorsDifferentLengths) == 6);
static_assert(static_cast<int>(manifold::Manifold::Error::MergeIndexOutOfBounds) == 7);
static_assert(static_cast<int>(manifold::Manifold::Error::TransformWrongLength) == 8);
static_assert(static_cast<int>(manifold::Manifold::Error::RunIndexWrongLength) == 9);
static_assert(static_cast<int>(manifold::Manifold::Error::FaceIDWrongLength) == 10);
static_assert(static_cast<int>(manifold::Manifold::Error::InvalidConstruction) == 11);
static_assert(static_cast<int>(manifold::Manifold::Error::ResultTooLarge) == 12);
static_assert(static_cast<int>(manifold::Manifold::Error::InvalidTangents) == 13);
static_assert(static_cast<int>(manifold::Manifold::Error::Cancelled) == 14);

namespace plinth::solid {
namespace {

std::atomic<std::uint64_t> get_mesh_gl64_calls{0};

struct ExpectedError {
  FfiErrorCode code;
  std::int32_t manifold_status = 0;
  bool has_mesh_index = false;
  std::uint64_t mesh_index = 0;
  FfiOutputLimit output_limit = FfiOutputLimit::Vertices;
  std::uint64_t output_count = 0;
};

FfiResponse error_response(FfiErrorCode code) noexcept {
  return FfiResponse{code, 0, false, 0, FfiOutputLimit::Vertices, 0,
                     FfiMesh{}};
}

FfiResponse error_response(const ExpectedError& error) noexcept {
  return FfiResponse{error.code, error.manifold_status, error.has_mesh_index,
                     error.mesh_index, error.output_limit, error.output_count,
                     FfiMesh{}};
}

void validate_offsets(rust::Slice<const double> vertices,
                      rust::Slice<const std::uint64_t> triangles,
                      rust::Slice<const std::uint64_t> vertex_offsets,
                      rust::Slice<const std::uint64_t> triangle_offsets) {
  if (vertex_offsets.size() != triangle_offsets.size() ||
      vertex_offsets.size() < 2) {
    throw ExpectedError{FfiErrorCode::OffsetArrayShape};
  }
  if (vertex_offsets[0] != 0 || triangle_offsets[0] != 0 ||
      vertex_offsets[vertex_offsets.size() - 1] != vertices.size() ||
      triangle_offsets[triangle_offsets.size() - 1] != triangles.size()) {
    throw ExpectedError{FfiErrorCode::OffsetSpan};
  }
  for (std::size_t i = 1; i < vertex_offsets.size(); ++i) {
    if (vertex_offsets[i] <= vertex_offsets[i - 1] ||
        triangle_offsets[i] <= triangle_offsets[i - 1] ||
        vertex_offsets[i] % 3 != 0 || triangle_offsets[i] % 3 != 0) {
      throw ExpectedError{FfiErrorCode::OffsetOrderOrAlignment};
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
      throw ExpectedError{FfiErrorCode::InputManifoldStatus,
                          static_cast<std::int32_t>(status), true,
                          static_cast<std::uint64_t>(mesh_index)};
    }
    solids.push_back(std::move(solid));
  }
  return solids;
}

FfiMesh export_result(manifold::Manifold result,
                      std::uint64_t max_output_vertices,
                      std::uint64_t max_output_triangles) {
  const auto status = result.Status();
  if (status != manifold::Manifold::Error::NoError) {
    throw ExpectedError{FfiErrorCode::ResultManifoldStatus,
                        static_cast<std::int32_t>(status)};
  }
  if (result.IsEmpty()) {
    throw ExpectedError{FfiErrorCode::EmptyResult};
  }
  const std::size_t num_vertices = result.NumVert();
  const std::size_t num_triangles = result.NumTri();
  if (num_vertices > max_output_vertices) {
    throw ExpectedError{FfiErrorCode::OutputLimitExceeded, 0, false, 0,
                        FfiOutputLimit::Vertices,
                        static_cast<std::uint64_t>(num_vertices)};
  }
  if (num_triangles > max_output_triangles) {
    throw ExpectedError{FfiErrorCode::OutputLimitExceeded, 0, false, 0,
                        FfiOutputLimit::Triangles,
                        static_cast<std::uint64_t>(num_triangles)};
  }

  get_mesh_gl64_calls.fetch_add(1, std::memory_order_relaxed);
  const manifold::MeshGL64 mesh = result.GetMeshGL64();
  if (mesh.numProp < 3) {
    throw ExpectedError{FfiErrorCode::OutputPropertyStride};
  }
  if (mesh.NumVert() != num_vertices || mesh.NumTri() != num_triangles ||
      mesh.triVerts.size() % 3 != 0) {
    throw ExpectedError{FfiErrorCode::OutputCountMismatch};
  }
  FfiMesh out;
  if (num_vertices > std::numeric_limits<std::size_t>::max() / 3) {
    throw ExpectedError{FfiErrorCode::OutputOverflow};
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

template <typename Operation>
FfiResponse execute(Operation&& operation) noexcept {
  try {
    return FfiResponse{FfiErrorCode::Success, 0, false, 0,
                       FfiOutputLimit::Vertices, 0,
                       operation()};
  } catch (const ExpectedError& error) {
    return error_response(error);
  } catch (const std::invalid_argument&) {
    return error_response(FfiErrorCode::UnexpectedInvalidArgument);
  } catch (const std::overflow_error&) {
    return error_response(FfiErrorCode::UnexpectedOverflow);
  } catch (const std::bad_alloc&) {
    return error_response(FfiErrorCode::UnexpectedAllocation);
  } catch (const std::exception&) {
    return error_response(FfiErrorCode::UnexpectedStandardException);
  } catch (...) {
    return error_response(FfiErrorCode::UnexpectedNonStandardException);
  }
}

}  // namespace

void reset_export_mesh_calls() noexcept {
  get_mesh_gl64_calls.store(0, std::memory_order_relaxed);
}

std::uint64_t export_mesh_calls() noexcept {
  return get_mesh_gl64_calls.load(std::memory_order_relaxed);
}

FfiResponse manifold_union_batch(rust::Slice<const double> vertices,
                                 rust::Slice<const std::uint64_t> triangles,
                                 rust::Slice<const std::uint64_t> vertex_offsets,
                                 rust::Slice<const std::uint64_t> triangle_offsets,
                                 std::uint64_t max_output_vertices,
                                 std::uint64_t max_output_triangles) noexcept {
  return execute([&] {
    auto solids =
        import_batch(vertices, triangles, vertex_offsets, triangle_offsets);
    return export_result(manifold::Manifold::BatchBoolean(
                             solids, manifold::OpType::Add),
                         max_output_vertices, max_output_triangles);
  });
}

FfiResponse manifold_difference_batch(
    rust::Slice<const double> vertices,
    rust::Slice<const std::uint64_t> triangles,
    rust::Slice<const std::uint64_t> vertex_offsets,
    rust::Slice<const std::uint64_t> triangle_offsets,
    std::uint64_t max_output_vertices,
    std::uint64_t max_output_triangles) noexcept {
  return execute([&] {
    auto solids =
        import_batch(vertices, triangles, vertex_offsets, triangle_offsets);
    if (solids.size() < 2) {
      throw ExpectedError{FfiErrorCode::DifferenceArity};
    }
    std::vector<manifold::Manifold> tail;
    tail.reserve(solids.size() - 1);
    for (std::size_t i = 1; i < solids.size(); ++i) {
      tail.push_back(std::move(solids[i]));
    }
    auto subtractor =
        manifold::Manifold::BatchBoolean(tail, manifold::OpType::Add);
    return export_result(
        solids.front().Boolean(subtractor, manifold::OpType::Subtract),
        max_output_vertices, max_output_triangles);
  });
}

// Each piece, trimmed to the kept side of its own plane, minus ONE shared
// solid -- the whole batch in one native call. Mesh 0 is the shared
// subtrahend; meshes 1..n are the pieces, and `planes` holds four doubles per
// piece, `(nx, ny, nz, offset)`, keeping `{p : n . p >= offset}`. A piece may
// come out EMPTY; its span in the output offsets is then zero-length.
FfiBatchResponse manifold_trim_subtract_each(
    rust::Slice<const double> vertices,
    rust::Slice<const std::uint64_t> triangles,
    rust::Slice<const std::uint64_t> vertex_offsets,
    rust::Slice<const std::uint64_t> triangle_offsets,
    rust::Slice<const double> planes,
    std::uint64_t max_output_vertices,
    std::uint64_t max_output_triangles) noexcept {
  const auto fail = [](const ExpectedError& error) {
    FfiBatchResponse response{};
    response.code = error.code;
    response.manifold_status = error.manifold_status;
    response.has_mesh_index = error.has_mesh_index;
    response.mesh_index = error.mesh_index;
    response.output_limit = error.output_limit;
    response.output_count = error.output_count;
    return response;
  };
  try {
    auto solids =
        import_batch(vertices, triangles, vertex_offsets, triangle_offsets);
    if (solids.size() < 2 || planes.size() != 4 * (solids.size() - 1)) {
      throw ExpectedError{FfiErrorCode::DifferenceArity};
    }
    const manifold::Manifold& subtrahend = solids.front();
    FfiBatchResponse response{};
    response.code = FfiErrorCode::Success;
    response.output_limit = FfiOutputLimit::Vertices;
    response.batch.vertex_offsets.push_back(0);
    response.batch.triangle_offsets.push_back(0);
    std::uint64_t total_vertices = 0;
    std::uint64_t total_triangles = 0;
    for (std::size_t piece = 1; piece < solids.size(); ++piece) {
      const double* plane = planes.data() + 4 * (piece - 1);
      const manifold::Manifold& whole = solids[piece];
      const manifold::Manifold kept =
          whole.TrimByPlane(manifold::vec3(plane[0], plane[1], plane[2]),
                            plane[3]);
      const manifold::Manifold result =
          kept.Boolean(subtrahend, manifold::OpType::Subtract);
      const auto status = result.Status();
      if (status != manifold::Manifold::Error::NoError) {
        throw ExpectedError{FfiErrorCode::ResultManifoldStatus,
                            static_cast<std::int32_t>(status)};
      }
      response.batch.volumes.push_back(whole.Volume());
      response.batch.volumes.push_back(kept.Volume());
      response.batch.volumes.push_back(result.Volume());
      if (!result.IsEmpty()) {
        get_mesh_gl64_calls.fetch_add(1, std::memory_order_relaxed);
        const manifold::MeshGL64 mesh = result.GetMeshGL64();
        if (mesh.numProp < 3) {
          throw ExpectedError{FfiErrorCode::OutputPropertyStride};
        }
        const std::size_t num_vertices = mesh.NumVert();
        total_vertices += num_vertices;
        total_triangles += mesh.NumTri();
        if (total_vertices > max_output_vertices) {
          throw ExpectedError{FfiErrorCode::OutputLimitExceeded, 0, false, 0,
                              FfiOutputLimit::Vertices, total_vertices};
        }
        if (total_triangles > max_output_triangles) {
          throw ExpectedError{FfiErrorCode::OutputLimitExceeded, 0, false, 0,
                              FfiOutputLimit::Triangles, total_triangles};
        }
        for (std::size_t vertex = 0; vertex < num_vertices; ++vertex) {
          const std::size_t base = vertex * mesh.numProp;
          response.batch.vertices.push_back(mesh.vertProperties[base]);
          response.batch.vertices.push_back(mesh.vertProperties[base + 1]);
          response.batch.vertices.push_back(mesh.vertProperties[base + 2]);
        }
        for (const std::uint64_t index : mesh.triVerts) {
          response.batch.triangles.push_back(index);
        }
      }
      response.batch.vertex_offsets.push_back(response.batch.vertices.size());
      response.batch.triangle_offsets.push_back(
          response.batch.triangles.size());
    }
    return response;
  } catch (const ExpectedError& error) {
    return fail(error);
  } catch (const std::invalid_argument&) {
    return fail(ExpectedError{FfiErrorCode::UnexpectedInvalidArgument});
  } catch (const std::overflow_error&) {
    return fail(ExpectedError{FfiErrorCode::UnexpectedOverflow});
  } catch (const std::bad_alloc&) {
    return fail(ExpectedError{FfiErrorCode::UnexpectedAllocation});
  } catch (const std::exception&) {
    return fail(ExpectedError{FfiErrorCode::UnexpectedStandardException});
  } catch (...) {
    return fail(ExpectedError{FfiErrorCode::UnexpectedNonStandardException});
  }
}

}  // namespace plinth::solid
