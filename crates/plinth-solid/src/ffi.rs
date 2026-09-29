// The only dead-in-production items in the bridge are the two export-mesh call
// counters, which exist for the `#[cfg(test)]` re-export below; narrow the
// blanket allow to non-test builds so a genuinely unused bridge item still
// warns under the test-target gates.
#[cfg_attr(not(test), allow(dead_code))]
#[cxx::bridge(namespace = "plinth::solid")]
mod bridge {
    struct FfiMesh {
        vertices: Vec<f64>,
        triangles: Vec<u64>,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FfiErrorCode {
        Success,
        OffsetArrayShape,
        OffsetSpan,
        OffsetOrderOrAlignment,
        DifferenceArity,
        OutputPropertyStride,
        OutputOverflow,
        OutputLimitExceeded,
        OutputCountMismatch,
        InputManifoldStatus,
        ResultManifoldStatus,
        EmptyResult,
        UnexpectedInvalidArgument,
        UnexpectedOverflow,
        UnexpectedAllocation,
        UnexpectedStandardException,
        UnexpectedNonStandardException,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FfiOutputLimit {
        Vertices,
        Triangles,
    }

    struct FfiResponse {
        code: FfiErrorCode,
        manifold_status: i32,
        has_mesh_index: bool,
        mesh_index: u64,
        output_limit: FfiOutputLimit,
        output_count: u64,
        mesh: FfiMesh,
    }

    /// Many result meshes in one flattened buffer. Piece `i` owns
    /// `vertices[vertex_offsets[i]..vertex_offsets[i + 1]]` and the matching
    /// triangle span, with triangle indices LOCAL to the piece. An empty span
    /// is an empty result. `volumes` holds three numbers per piece: the whole
    /// piece, after its plane trim, and after the subtraction.
    struct FfiMeshBatch {
        vertices: Vec<f64>,
        triangles: Vec<u64>,
        vertex_offsets: Vec<u64>,
        triangle_offsets: Vec<u64>,
        volumes: Vec<f64>,
    }

    struct FfiBatchResponse {
        code: FfiErrorCode,
        manifold_status: i32,
        has_mesh_index: bool,
        mesh_index: u64,
        output_limit: FfiOutputLimit,
        output_count: u64,
        batch: FfiMeshBatch,
    }

    unsafe extern "C++" {
        include!("plinth-solid/src/manifold_bridge.h");

        #[allow(clippy::too_many_arguments)]
        fn manifold_trim_subtract_each(
            vertices: &[f64],
            triangles: &[u64],
            vertex_offsets: &[u64],
            triangle_offsets: &[u64],
            planes: &[f64],
            max_output_vertices: u64,
            max_output_triangles: u64,
        ) -> FfiBatchResponse;

        fn manifold_union_batch(
            vertices: &[f64],
            triangles: &[u64],
            vertex_offsets: &[u64],
            triangle_offsets: &[u64],
            max_output_vertices: u64,
            max_output_triangles: u64,
        ) -> FfiResponse;

        fn manifold_difference_batch(
            vertices: &[f64],
            triangles: &[u64],
            vertex_offsets: &[u64],
            triangle_offsets: &[u64],
            max_output_vertices: u64,
            max_output_triangles: u64,
        ) -> FfiResponse;

        fn reset_export_mesh_calls();

        fn export_mesh_calls() -> u64;
    }
}

pub(crate) use bridge::{
    manifold_difference_batch, manifold_trim_subtract_each, manifold_union_batch, FfiErrorCode,
    FfiMesh, FfiMeshBatch, FfiOutputLimit, FfiResponse,
};

#[cfg(test)]
pub(crate) use bridge::{export_mesh_calls, reset_export_mesh_calls};

#[cfg(test)]
mod tests {
    use super::{manifold_union_batch, FfiErrorCode};

    #[test]
    fn malformed_native_offsets_return_a_typed_code_without_throwing() {
        let response = manifold_union_batch(&[], &[], &[0], &[0], u64::MAX, u64::MAX);
        assert_eq!(response.code, FfiErrorCode::OffsetArrayShape);
    }
}
