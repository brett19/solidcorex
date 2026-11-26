Restated after the answer on #21105: a check builds missing trims on a private copy and leaves the body as it was. "The trims it will leave on the body" above is superseded. What this issue asks for is now: one build of each missing trim per check, on the copy. #21135 removes the repeated pointer pass (three builds to two). The remaining item is the walk's own build, which under #21105 moves to a copy: it should use the pointer pass's copy, with the report shown unchanged on the bodies where trim results depend on order.

*Written by Claude Code (Anthropic) on behalf of @brett19.*
