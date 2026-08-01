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
        InputManifoldStatus,
        ResultManifoldStatus,
        EmptyResult,
        UnexpectedInvalidArgument,
        UnexpectedOverflow,
        UnexpectedAllocation,
        UnexpectedStandardException,
        UnexpectedNonStandardException,
    }

    struct FfiResponse {
        code: FfiErrorCode,
        manifold_status: i32,
        has_mesh_index: bool,
        mesh_index: u64,
        mesh: FfiMesh,
    }

    unsafe extern "C++" {
        include!("plinth-solid/src/manifold_bridge.h");

        fn manifold_union_batch(
            vertices: &[f64],
            triangles: &[u64],
            vertex_offsets: &[u64],
            triangle_offsets: &[u64],
        ) -> FfiResponse;

        fn manifold_difference_batch(
            vertices: &[f64],
            triangles: &[u64],
            vertex_offsets: &[u64],
            triangle_offsets: &[u64],
        ) -> FfiResponse;
    }
}

pub(crate) use bridge::{
    manifold_difference_batch, manifold_union_batch, FfiErrorCode, FfiMesh, FfiResponse,
};

#[cfg(test)]
mod tests {
    use super::{manifold_union_batch, FfiErrorCode};

    #[test]
    fn malformed_native_offsets_return_a_typed_code_without_throwing() {
        let response = manifold_union_batch(&[], &[], &[0], &[0]);
        assert_eq!(response.code, FfiErrorCode::OffsetArrayShape);
    }
}
