# ReflectionClass::getTraitAliases (PHP 8.5)

## Goal
- Track trait aliases from `use Trait { method as alias; }` and expose them through `ReflectionClass::getTraitAliases()`.

## References
- `~/php-src/ext/reflection/php_reflection.c` (ReflectionClass::getTraitAliases)

## Plan
1. Add trait alias metadata to `ClassDef` for alias name, source trait, method, and visibility.
2. Emit opcodes for trait alias adaptations during class compilation.
3. Store trait alias metadata in the VM while defining the class.
4. Implement `ReflectionClass::getTraitAliases()` to return alias => `Trait::method`.
5. Add integration tests that exercise alias resolution with and without explicit trait names.
