One correction to the cost of (a), read from main; it does not argue against (a), it adds a condition to step 1.

"`solidcore-ops` (`Body::check`) already checks a copy, so ops callers never got the saving" holds for `Body::check` only. The outward-solid proof runs the same check, at level 0, on the body itself:

- `valid_and_outward_failure(k, b: &mut Brep)` calls `check_brep(k, b, Level0)` (`solidcore/src/topology/brep.rs`);
- `prove_naming` (`solidcore-ops/src/body.rs`) calls it on the result of every solid operation, through `finish_proved`;
- `Expect::of(k, input: &mut Brep)` calls `is_outward_solid` on an operation's input.

Level 0 builds the same trims as level 2 (measured: same counts and times). So today the first proof of a body leaves its trims on it, and every later proof and operation on that body finds them. A body made by `Body::from_record` has no trims (only `to_record` calls `create_missing_uv_trim_curves`).

Under (a) with nothing else changed, every proof of a body that lacks trims would build them again on a copy: on a body like Fillet/blade_hub_1 that is at least 1.2 s per proof where a repeat costs 0.034 s today, on each operation that proves that body, until some operation happens to create them.

So step 1 needs, in the same change or before it:

1. the ops layer creating a body's missing trims once, as the visible operation the answer describes, at the point a body enters without them and before its proof (the raises then happen once, in one defined order, instead of in whichever check ran first);
2. counts in the PR for the proof path: trim builds and bound computations per proof, first and repeat, on a body from a record and on an operation result, before and after. A repeat proof must build none.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
