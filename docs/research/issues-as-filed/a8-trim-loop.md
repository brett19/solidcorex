Two places outside the kernel each carry their own copy of the same step: before a body is written out, give every edge use on a face that has no UV trim curve the kernel's own.

- `solidcore-convert/src/brep/record.rs`, at the start of `to_record`;
- `crates/smlib/src/core/convert.rs`, at the start of `flat_from_brep`.

Both walk `topology::brep::p2::edgeuses(b, None)`, skip an edge use with no face use or with a trim already, and call `edgeuse::p3::get_or_create_uv_trim_curve(k, b, eu, false, false, true)`, failing on the first status that is an error. The two loops differ only in how they wrap the error.

`docs/layers.md` treats a kernel step written in two layers as a defect: the walk order and the skip rule decide which trims a body gets and at which tolerance, so the two copies can drift apart.

## Proposed change

One kernel function that does this step, called by both. No result changes: the function is the loop both callers already run, in the same order.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
