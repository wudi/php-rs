# Readonly Class Support (PHP 8.5)

## Goal
- Support readonly classes, enforcing PHP 8.2+ semantics and exposing `ReflectionClass::isReadOnly()`.

## References
- `~/php-src/Zend/zend_compile.c` (readonly class flags and validation)
- `~/php-src/Zend/tests/readonly_classes` (behavioral expectations)

## Plan
1. Add `is_readonly` to `ClassDef` and plumb it through class creation.
2. Emit and handle a class opcode to mark classes as readonly.
3. Enforce readonly class rules in the VM (inheritance mismatch, static properties, AllowDynamicProperties).
4. Ensure instance properties inherit readonly when the class is readonly.
5. Implement `ReflectionClass::isReadOnly()` and add tests for reflection + basic enforcement.
