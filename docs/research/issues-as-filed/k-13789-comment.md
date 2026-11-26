Measurements that bear on this, from regenerating 63 designs (the recipes under `examples/`, the engine fixture designs and 11 STEP parts) through the app's export: 474 record exports, 7,465 faces, 17,724 edges.

- One face was written as a stand-in for an offset surface: the Thicken face of `examples/regressions/fins_joined_into_their_hub.json`. Its spline lies at most 1.03e-6 from the face (face tolerance 1e-5); the body's volume differs from its record's import by 4.7e-7 relative. It was the only body of the 474 whose volume differed from its record's by more than 1e-12.
- No procedural curve reached an export. 158 blend faces (16 corner, 142 round on an analytic net) were written as their net without a definition; no point moved.
- The stand-in is not always inside the face's tolerance: #21926 has a case at 21 times it.
- `docs/native-brep.md` does not mention that the app's export approximates these kinds.

Thicken, Shell and Offset are the features that make the kind.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
