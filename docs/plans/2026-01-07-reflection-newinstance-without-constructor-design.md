# ReflectionClass::newInstanceWithoutConstructor Design

## Goal
Match PHP behavior for `ReflectionClass::newInstanceWithoutConstructor()` by instantiating
objects without invoking constructors, while preserving PHP error semantics.

## Architecture
Add a small helper in `src/builtins/reflection.rs` that performs the PHP guard for internal,
final classes with custom allocators, then delegates to the VM object creation path.
We will not bypass the VM's instantiation checks so that enum/abstract/interface/trait
errors match existing runtime behavior.

## Data Flow
The reflection builtin resolves the target `ClassDef` as it does for other reflection
methods, then:
1. Validates the "internal final class with create_object" restriction.
2. Calls the standard object initialization routine without invoking constructors.
3. Returns the created object handle or lets the VM propagate instantiation errors.

## Error Handling
If the target class is internal, final, and has a custom allocator, throw a
`ReflectionException` with the exact PHP error string. Other instantiation errors
are left to the VM, preserving core messaging for enums, abstract classes, interfaces,
and traits.

## Testing
Add unit tests in `tests/reflection_test.rs`:
- User-defined class returns object without running `__construct`.
- Enum instantiation throws the "Cannot instantiate enum Foo" error.
- Abstract/interface/trait instantiation errors match core VM behavior.
- Internal final class (e.g., `Generator`) throws the PHP reflection exception message.
