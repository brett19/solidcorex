//! Design-study prototype for an immutable B-rep body with cheap edits (research report 51).
//! Written from scratch; std only. See README.md.

pub mod body;
pub mod counting;
pub mod ids;
pub mod model;
pub mod table;

/// Compile-time proof that every finished body layout is `Send + Sync`.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    use body::*;
    assert_send_sync::<TableBody<PlainF>>();
    assert_send_sync::<TableBody<WholeF>>();
    assert_send_sync::<TableBody<ChunkF>>();
    assert_send_sync::<TableBody<TrieF>>();
    assert_send_sync::<FaceBody<PlainF>>();
    assert_send_sync::<FaceBody<ChunkF>>();
    assert_send_sync::<FaceBody<Chunk16F>>();
    assert_send_sync::<FaceBody<TrieF>>();
};
