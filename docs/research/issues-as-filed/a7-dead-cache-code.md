Two pieces of cache code in the kernel do nothing:

- `topology::brep::p2::BrepCacheSlot` is the caller-supplied B-rep spatial cache that `notify` keeps in step with edits. Nothing constructs one: every caller of `notify` and `notify_with_body_identity` passes `None`, so the cache-maintenance arms of `notify` (`PreEdit`, `MergeInBrep`, `RmFromBrep`, `SplitInBrep`, `AddToBrep`) never run.
- `delete_stale_caches` in `boolean/merge/p4.rs`, called at the end of a Boolean, has an empty effect.

Both read as live machinery and suggest invalidation that does not happen.

## Proposed change

Remove the slot parameter and its arms from `notify`, and remove the empty step. No result changes; the goldens units that exercise `notify` keep their rows.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
