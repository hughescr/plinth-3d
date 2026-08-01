#[cxx::bridge(namespace = "plinth::solid")]
mod bridge {
    struct FfiMesh {
        vertices: Vec<f64>,
        triangles: Vec<u64>,
    }

    struct BackendConfig {
        version_major: u32,
        version_minor: u32,
        version_patch: u32,
        parallel_mode: i32,
        scalar_bytes: u32,
        index_bytes: u32,
        cross_section_enabled: bool,
    }

    unsafe extern "C++" {
        include!("plinth-solid/src/manifold_bridge.h");

        fn manifold_config() -> BackendConfig;

        fn manifold_union_batch(
            vertices: &[f64],
            triangles: &[u64],
            vertex_offsets: &[u64],
            triangle_offsets: &[u64],
        ) -> Result<FfiMesh>;

        fn manifold_difference_batch(
            vertices: &[f64],
            triangles: &[u64],
            vertex_offsets: &[u64],
            triangle_offsets: &[u64],
        ) -> Result<FfiMesh>;
    }
}

pub(crate) use bridge::{
    manifold_config, manifold_difference_batch, manifold_union_batch, FfiMesh,
};
