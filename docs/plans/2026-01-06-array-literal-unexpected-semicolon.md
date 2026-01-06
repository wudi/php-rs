# Plan: Array literal unexpected semicolon recovery

## Goal
- Match PHP behavior for `var_dump([1;]);` by emitting a parse error instead of looping/oom.

## Reference
- Native PHP: `PHP Parse error:  syntax error, unexpected token ";", expecting "]"`.

## Steps
1. Add a parser recovery test that covers `[1;]` and asserts an error is reported.
2. Update array/list parsing loops to consume a stray `;` inside array items and record a parse error.
3. Verify parsing terminates and the test passes.
