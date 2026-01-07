# Reflection Property/Constant Doc Comments (PHP 8.5)

## Goal
- Track doc comments for class properties and class constants so reflection APIs return the PHP 8.5 values.

## References
- `~/php-src/ext/reflection/php_reflection.c` (ReflectionProperty::getDocComment, ReflectionClassConstant::getDocComment)

## Plan
1. Extend runtime metadata for properties and class constants to store doc comments.
2. Emit doc comment opcodes for property and class constant declarations, including multi-entry declarations.
3. Update VM opcode handlers to persist doc comments into metadata.
4. Update reflection methods to return the stored doc comments (or false).
5. Add integration tests for property and class constant doc comments.
