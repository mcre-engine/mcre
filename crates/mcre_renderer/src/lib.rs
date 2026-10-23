pub mod atlas;
pub mod camera;
pub mod clouds;
pub mod fluid;
pub mod frustum;
pub mod lightmap;
pub mod math;
pub mod mesh;
pub mod occlusion;
pub mod region;
pub mod remesh;
pub mod renderer;
pub mod vertex;

pub use atlas::{Atlas, AtlasError, AtlasRegion, build_atlas};
pub use camera::FlyCamera;
pub use clouds::{
    CLOUD_HEIGHT, CloudCell, CloudOrigin, CloudStatus, CloudTexture, CloudVertex,
    RelativeCameraPos, build_cloud_mesh, cloud_fog_end, cloud_origin,
};
pub use frustum::Frustum;
pub use math::Mat4;
pub use mesh::{
    BlockAccess, LightMode, Mesh, MeshOptions, QuadMeta, QuadSortData, SectionMesh, SectionPass,
    Vertex, mesh_chunk, pack_chunk_sections,
};
pub use occlusion::{
    CullParams, FULL_VISIBILITY, SectionVisibility, compute_section_visibility,
    find_visible_sections,
};
pub use region::{
    RegionKey, RenderRegion, SectionAlloc, SectionPassAlloc, SharedQuadIndexBuffer,
    chunk_section_slot, local_section_index, merge_visible_runs, region_key, section_world_origin,
    slice_mask, sort_translucent_quads, visible_faces,
};
pub use remesh::{MeshJob, MeshResult, MeshWorker, MeshedChunk, StreamSource, run_job};
pub use renderer::{DebugFlags, EguiFrame, Fog, FrameStats, RenderChunk, Renderer, RendererError};
pub use vertex::{
    FACING_COUNT, FACING_NEG_X, FACING_NEG_Y, FACING_NEG_Z, FACING_POS_X, FACING_POS_Y,
    FACING_POS_Z, FACING_UNASSIGNED, FormatFlags, QuadEncodeInput, TerrainPass, TerrainVertex,
    encode_quad, facing_of, facing_of_cullface, pack_local_section_index,
};
