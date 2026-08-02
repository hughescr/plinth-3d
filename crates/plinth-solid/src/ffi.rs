#[allow(dead_code)]
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

    unsafe extern "C++" {
        include!("plinth-solid/src/manifold_bridge.h");

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
    manifold_difference_batch, manifold_union_batch, FfiErrorCode, FfiMesh, FfiOutputLimit,
    FfiResponse,
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
