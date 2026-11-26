`solidcore-ops` has three ways to run a call on a body: `with_body` for edits, `read_body` for reads, and `measure_body` for measures that need a writable body. Most of the read-only measures in `measure.rs` use `with_body`.

`with_body` is built for edits. It takes the kernel's snapshot of the whole body first (`Brep::with_rollback`), so that a refused or failed edit can be undone, and it hands out the body mutably, which drops what is kept beside the body for as long as it is unchanged (the orientation proof and the template). A read that goes through it therefore copies the body and discards the proof, although it changes nothing a caller can see.

About thirty measures do this, among them `is_manifold_solid`, `solid_count`, `rim_edges`, `vertex_distance`, `face_tolerance`, `edge_length`, `bounds`, `area`, `mass_properties`, the point classifications and the ray queries. Several of them never use the kernel or the body mutably at all (their closures ignore the kernel and only read the body).

## Proposed change

Each measure uses the door that matches what it does:

1. A measure whose kernel calls take the body by shared reference moves to `read_body`: no snapshot, and the proof stays. These are identified by the compiler: the closure builds against `&Brep`.
2. The measures whose kernel calls still take the body mutably stay where they are for now. They move as those kernel calls stop needing a writable body.

Step 1 changes no answer: the same kernel call runs on the same body. It removes one whole-body snapshot per call and keeps the proof valid across reads.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
