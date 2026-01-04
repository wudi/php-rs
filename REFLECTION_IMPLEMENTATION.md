# PHP Reflection API Implementation

## Overview

This implementation provides a PHP Reflection API for the php-rs interpreter, enabling runtime introspection of classes, functions, methods, properties, and parameters.

## 📊 Implementation Status

**Current Progress:** ~28% of full PHP Reflection API

| Category | Implemented | Total | Percentage |
|----------|-------------|-------|------------|
| **Classes** | 4 | 25 | **16%** |
| **ReflectionClass methods** | 22 | ~60 | **37%** |
| **ReflectionFunction methods** | 13 | ~20 | **65%** |
| **ReflectionMethod methods** | 13 | ~15 | **87%** |
| **ReflectionParameter methods** | 9 | ~20 | **45%** |
| **Total methods (estimated)** | 57 | ~200+ | **28%** |

## ✅ Implemented Classes (4/25)

### ReflectionClass (22/60 methods - 37% complete)

A class for inspecting PHP classes and their metadata.

**✅ Methods Implemented (22):**
- `__construct(string|object $objectOrClass)` - Create reflection from class name or object instance
- `getName(): string` - Get the name of the class
- `isAbstract(): bool` - Check if the class is abstract
- `isFinal(): bool` - Check if the class is final (placeholder)
- `isInterface(): bool` - Check if the class is an interface
- `isTrait(): bool` - Check if the class is a trait
- `isEnum(): bool` - Check if the class is an enum
- `isInstantiable(): bool` - Check if the class can be instantiated
- `hasMethod(string $name): bool` - Check if a method exists
- `hasProperty(string $name): bool` - Check if a property exists
- `hasConstant(string $name): bool` - Check if a constant exists
- `getMethods(?int $filter = null): array` - Get all methods
- `getProperties(?int $filter = null): array` - Get all properties
- `getConstants(): array` - Get all constants
- `getConstant(string $name): mixed` - Get a specific constant value
- `getParentClass(): ReflectionClass|false` - Get parent class reflection
- `getInterfaceNames(): array` - Get names of implemented interfaces
- `implementsInterface(ReflectionClass|string $interface): bool` - Check interface implementation
- `getNamespaceName(): string` - Get the namespace name
- `getShortName(): string` - Get the class name without namespace
- `inNamespace(): bool` - Check if the class is namespaced
- `__toString(): string` - String representation of the class

**❌ Missing Methods (~38):**
- `export()` - Static export method
- `getAttributes()` - PHP 8.0+ attributes
- `getConstructor()` - Get constructor method as ReflectionMethod
- `getDefaultProperties()` - Get default property values
- `getDocComment()` - Get doc comments
- `getEndLine()` / `getStartLine()` - Get line numbers in source
- `getExtension()` / `getExtensionName()` - Extension that defined the class
- `getFileName()` - Get source file path
- `getInterfaces()` - Get interface ReflectionClass objects
- `getMethod(string $name)` - Get single ReflectionMethod object
- `getModifiers()` - Get modifier flags (public/private/abstract/final)
- `getProperty(string $name)` - Get single ReflectionProperty object
- `getReflectionConstant()` / `getReflectionConstants()` - Get ReflectionClassConstant objects
- `getStaticProperties()` / `getStaticPropertyValue()` / `setStaticPropertyValue()` - Static property access
- `getTraitAliases()` / `getTraitNames()` / `getTraits()` - Trait introspection
- `isAnonymous()` - Check if anonymous class
- `isCloneable()` - Check if cloneable
- `isInstance(object $object)` - Check if object is instance of this class
- `isInternal()` / `isUserDefined()` - Check if built-in or user-defined
- `isIterable()` / `isIterateable()` - Check if iterable
- `isReadOnly()` - Check if readonly class (PHP 8.2+)
- `isSubclassOf()` - Check subclass relationship
- `newInstance()` / `newInstanceArgs()` / `newInstanceWithoutConstructor()` - Dynamic instantiation
- Lazy object methods (PHP 8.4+): `getLazyInitializer()`, `initializeLazyObject()`, `isUninitializedLazyObject()`, `markLazyObjectAsInitialized()`, `newLazyGhost()`, `newLazyProxy()`, `resetAsLazyGhost()`, `resetAsLazyProxy()`

### ReflectionFunction (13/20 methods - 65% complete)

A class for inspecting PHP functions.

**✅ Methods Implemented (13):**
- `__construct(string $name)` - Create reflection from function name
- `getName(): string` - Get the name of the function
- `getNumberOfParameters(): int` - Get total parameter count
- `getNumberOfRequiredParameters(): int` - Get required parameter count
- `getParameters(): array` - Get array of ReflectionParameter objects
- `isUserDefined(): bool` - Check if function is user-defined
- `isInternal(): bool` - Check if function is built-in
- `isVariadic(): bool` - Check if function accepts variable arguments
- `returnsReference(): bool` - Check if function returns by reference
- `getNamespaceName(): string` - Get the namespace name
- `getShortName(): string` - Get function name without namespace
- `inNamespace(): bool` - Check if function is in a namespace
- `isClosure(): bool` - Check if function is a closure (placeholder returns false)
- `isGenerator(): bool` - Check if function is a generator

**❌ Missing Methods (~7):**
- `export()` - Static export
- `getClosure()` - Get closure representation
- `invoke(...$args)` / `invokeArgs(array $args)` - Dynamic invocation
- `isAnonymous()` - Check if anonymous function
- `isDisabled()` - Check if function is disabled
- `getDocComment()` - Get doc comments
- `getFileName()` / `getStartLine()` / `getEndLine()` - Source location methods

### ReflectionMethod (13/15 methods - 87% complete)

A class for inspecting class methods.

**✅ Methods Implemented (13):**
- `__construct(string|object $class, string $method)` - Create reflection from class and method name
- `getName(): string` - Get the name of the method
- `getDeclaringClass(): ReflectionClass` - Get the class that declares this method
- `getModifiers(): int` - Get method modifier flags (public/private/protected/static/abstract)
- `isPublic(): bool` - Check if method is public
- `isPrivate(): bool` - Check if method is private
- `isProtected(): bool` - Check if method is protected
- `isAbstract(): bool` - Check if method is abstract
- `isFinal(): bool` - Check if method is final (placeholder)
- `isStatic(): bool` - Check if method is static
- `isConstructor(): bool` - Check if method is __construct
- `isDestructor(): bool` - Check if method is __destruct
- `__toString(): string` - String representation

**❌ Missing Methods (~2):**
- `invoke(object $object, mixed ...$args)` - Invoke the method
- `invokeArgs(object $object, array $args)` - Invoke with arguments array
- Plus inherited methods from ReflectionFunctionAbstract

### ReflectionParameter (9/20 methods - 45% complete)

A class for inspecting function/method parameters.

**✅ Methods Implemented (9):**
- `__construct(string|array $function, int|string $param)` - Create reflection from function/method and parameter
- `getName(): string` - Get the parameter name
- `isOptional(): bool` - Check if parameter is optional (has default value)
- `isVariadic(): bool` - Check if parameter is variadic (...)
- `isPassedByReference(): bool` - Check if parameter is passed by reference (&)
- `hasType(): bool` - Check if parameter has a type declaration
- `allowsNull(): bool` - Check if parameter allows null values
- `getDefaultValue(): mixed` - Get the default value if available
- `isDefaultValueAvailable(): bool` - Check if default value is available

**❌ Missing Methods (~11):**
- `canBePassedByValue()` - Check if can be passed by value
- `getAttributes()` - Get parameter attributes (PHP 8.0+)
- `getClass()` - Get parameter class (deprecated)
- `getDeclaringClass()` - Get declaring class for method parameters
- `getDeclaringFunction()` - Get declaring function as ReflectionFunctionAbstract
- `getDefaultValueConstantName()` - Get constant name if default is constant
- `getPosition()` - Get parameter position (0-based)
- `getType()` - Get ReflectionType object
- `isArray()` - Check if parameter is array (deprecated)
- `isCallable()` - Check if parameter is callable (deprecated)
- `isDefaultValueConstant()` - Check if default value is a constant
- `isPromoted()` - Check if parameter is promoted property (PHP 8.0+)

## ❌ Not Yet Implemented Classes (21/25)

### High Priority Classes

1. **ReflectionProperty** - Property introspection (~30 methods)
   - Methods: `getAttributes()`, `getDeclaringClass()`, `getDefaultValue()`, `getDocComment()`, `getHook()`, `getHooks()`, `getModifiers()`, `getName()`, `getRawValue()`, `getSettableType()`, `getType()`, `getValue()`, `hasDefaultValue()`, `hasHook()`, `hasHooks()`, `hasType()`, `isAbstract()`, `isDefault()`, `isDynamic()`, `isFinal()`, `isInitialized()`, `isLazy()`, `isPrivate()`, `isPrivateSet()`, `isPromoted()`, `isProtected()`, `isProtectedSet()`, `isPublic()`, `isReadOnly()`, `isStatic()`, `isVirtual()`, `setAccessible()`, `setRawValue()`, `setRawValueWithoutLazyInitialization()`, `setValue()`, `skipLazyInitialization()`

2. **ReflectionNamedType** - Named type introspection (~2 methods)
   - Extends ReflectionType
   - Methods: `getName()`, `isBuiltin()`

3. **ReflectionType** - Type introspection base (~3 methods)
   - Methods: `allowsNull()`, `__toString()`

4. **ReflectionObject** - Object introspection (~2 methods)
   - Extends ReflectionClass
   - Specialized for object instances

### Medium Priority Classes

7. **ReflectionClassConstant** - Class constant introspection (~15 methods)
   - Methods: `getAttributes()`, `getDeclaringClass()`, `getDocComment()`, `getModifiers()`, `getName()`, `getType()`, `getValue()`, `hasType()`, `isDeprecated()`, `isEnumCase()`, `isFinal()`, `isPrivate()`, `isProtected()`, `isPublic()`

8. **ReflectionConstant** - Global constant introspection (~10 methods)
   - Methods: `getExtension()`, `getExtensionName()`, `getFileName()`, `getName()`, `getNamespaceName()`, `getShortName()`, `getValue()`, `isDeprecated()`

9. **ReflectionUnionType** - Union type introspection (~1 method)
   - Extends ReflectionType
   - Methods: `getTypes()`

10. **ReflectionIntersectionType** - Intersection type introspection (~1 method)
    - Extends ReflectionType
    - Methods: `getTypes()`

11. **ReflectionEnum** - Enum introspection (~4 methods)
    - Extends ReflectionClass
    - Methods: `getBackingType()`, `getCase()`, `getCases()`, `hasCase()`, `isBacked()`

12. **ReflectionEnumUnitCase** - Enum case introspection (~3 methods)
    - Extends ReflectionClassConstant
    - Methods: `getEnum()`, `getValue()`

13. **ReflectionEnumBackedCase** - Backed enum case introspection (~1 method)
    - Extends ReflectionEnumUnitCase
    - Methods: `getBackingValue()`

### Lower Priority Classes

14. **Reflection** - Base class with static methods (~2 methods)
    - Methods: `export()`, `getModifierNames()`

15. **ReflectionFunctionAbstract** - Abstract base for functions/methods (~30 methods)
    - Base class for ReflectionFunction and ReflectionMethod
    - See methods listed under ReflectionFunction missing methods

16. **ReflectionExtension** - Extension introspection (~12 methods)
    - Methods: `getClasses()`, `getClassNames()`, `getConstants()`, `getDependencies()`, `getFunctions()`, `getINIEntries()`, `getName()`, `getVersion()`, `info()`, `isPersistent()`, `isTemporary()`

17. **ReflectionZendExtension** - Zend extension introspection (~6 methods)
    - Methods: `getAuthor()`, `getCopyright()`, `getName()`, `getURL()`, `getVersion()`

18. **ReflectionGenerator** - Generator introspection (~7 methods)
    - Methods: `getExecutingFile()`, `getExecutingGenerator()`, `getExecutingLine()`, `getFunction()`, `getThis()`, `getTrace()`, `isClosed()`

19. **ReflectionFiber** - Fiber introspection (~5 methods)
    - Methods: `getCallable()`, `getExecutingFile()`, `getExecutingLine()`, `getFiber()`, `getTrace()`

20. **ReflectionAttribute** - Attribute introspection (~5 methods)
    - Methods: `getArguments()`, `getName()`, `getTarget()`, `isRepeated()`, `newInstance()`

21. **ReflectionReference** - Reference introspection (~2 methods)
    - Methods: `fromArrayElement()`, `getId()`

22. **Reflector** - Interface (~1 method)
    - Interface defining: `export()`

23. **ReflectionException** - Exception class
    - Standard exception for reflection errors

## 🎯 Implementation Roadmap

### Phase 1: Core Method Introspection (Priority: HIGH)
**Goal:** Enable full method and parameter reflection
- [ ] ReflectionMethod (15 methods)
- [ ] ReflectionParameter (20 methods)
- [ ] ReflectionFunctionAbstract enhancements
- [ ] ReflectionFunction completion (18 more methods)

### Phase 2: Property and Type System (Priority: HIGH)
**Goal:** Enable property and type introspection
- [ ] ReflectionProperty (30 methods)
- [ ] ReflectionType (3 methods)
- [ ] ReflectionNamedType (2 methods)
- [ ] ReflectionUnionType (1 method)
- [ ] ReflectionIntersectionType (1 method)

### Phase 3: Constants and Advanced Features (Priority: MEDIUM)
**Goal:** Complete class introspection capabilities
- [ ] ReflectionClassConstant (15 methods)
- [ ] ReflectionConstant (10 methods)
- [ ] ReflectionObject (2 methods)
- [ ] Complete ReflectionClass missing methods (~38 methods)

### Phase 4: Enums and Modern PHP Features (Priority: MEDIUM)
**Goal:** Support PHP 8.x features
- [ ] ReflectionEnum (4 methods)
- [ ] ReflectionEnumUnitCase (3 methods)
- [ ] ReflectionEnumBackedCase (1 method)
- [ ] ReflectionAttribute (5 methods)

### Phase 5: Extensions and Advanced Runtime (Priority: LOW)
**Goal:** Complete the API for advanced use cases
- [ ] ReflectionExtension (12 methods)
- [ ] ReflectionZendExtension (6 methods)
- [ ] ReflectionGenerator (7 methods)
- [ ] ReflectionFiber (5 methods)
- [ ] ReflectionReference (2 methods)
- [ ] Reflection static class (2 methods)
- [ ] Reflector interface
- [ ] ReflectionException

## Architecture

### Internal Data Storage

Reflection objects store their metadata as properties on the object itself:
- Class/function name stored as a "name" property
- Accessed through `Val::Object(handle)` → `Val::ObjPayload(ObjectData)`

### Extension Registration

The reflection extension is registered as a core extension through the `ReflectionExtension` struct, which implements the `Extension` trait with lifecycle hooks:
- `module_init()` - Registers all reflection classes during engine startup
- `request_init()` / `request_shutdown()` - Per-request initialization/cleanup
- `module_shutdown()` - Cleanup on engine shutdown

### Helper Functions

Internal helpers to manage common operations:
- `get_class_def()` - Retrieve ClassDef from VM context
- `lookup_symbol()` - Resolve Symbol to byte slice safely
- `get_reflection_class_name()` - Extract class name from ReflectionClass object
- `type_hint_to_string()` - Convert TypeHint enum to string representation

## Testing

Comprehensive test suite with 62 passing tests covering:

### ReflectionClass Tests (29 tests)
- Basic instantiation from class name and object instance
- Type checking (abstract, interface, trait, enum, instantiable)
- Member existence checks (methods, properties, constants)
- Member retrieval (getMethods, getProperties, getConstants)
- Constant value retrieval
- Parent class introspection
- Interface implementation checking
- Namespace handling
- String representation

### ReflectionMethod Tests (10 tests)
- Basic method reflection from class and method names
- Method reflection from object instances
- Declaring class retrieval
- Modifier and visibility checks (public, private, protected, abstract, static, final)
- Constructor and destructor detection
- String representation
- Error handling for non-existent methods

### ReflectionParameter Tests (10 tests)
- Parameter reflection by name and index
- Parameter reflection from methods
- Optional and required parameter detection
- Variadic parameter detection
- Pass-by-reference detection
- Type hint detection
- Null type allowance
- Default value retrieval
- Default value availability checking

### ReflectionFunction Tests (13 tests)
- Basic function reflection
- Parameter counting (total and required)
- Parameter array retrieval with ReflectionParameter objects
- User-defined vs internal function detection
- Variadic function detection
- Reference return detection
- Namespace handling (namespace name, short name, in namespace check)
- Closure detection
- Generator detection
- Built-in function reflection
- Error handling for non-existent functions

## Usage Examples

```php
<?php
// Reflect on a class
class MyClass {
    const VERSION = '1.0';
    public $name;
    public function greet() {}
}

$rc = new ReflectionClass('MyClass');
echo $rc->getName();              // "MyClass"
echo $rc->isAbstract();           // false
echo $rc->hasMethod('greet');     // true
echo $rc->hasProperty('name');    // true
echo $rc->getConstant('VERSION'); // "1.0"

// Reflect on an object instance
$obj = new MyClass();
$rc = new ReflectionClass($obj);
echo $rc->getName(); // "MyClass"

// Reflect on a function
function myFunction() {}
$rf = new ReflectionFunction('myFunction');
echo $rf->getName(); // "myFunction"

// Reflect on built-in functions
$rf = new ReflectionFunction('strlen');
echo $rf->getName(); // "strlen"
```

## Files Modified

1. **src/builtins/reflection.rs** (NEW) - Complete reflection implementation
2. **src/builtins/mod.rs** - Added reflection module export
3. **src/runtime/context.rs** - Registered ReflectionExtension in core extensions
4. **tests/reflection_test.rs** (NEW) - Comprehensive test suite

## Future Enhancements

The following Reflection classes can be added following the same pattern:

- **ReflectionMethod** - Method introspection with parameter and return type info
- **ReflectionProperty** - Property introspection with type and visibility info
- **ReflectionParameter** - Parameter introspection with type hints and default values
- **ReflectionType** - Type hint introspection
- **ReflectionClassConstant** - Class constant introspection with modifiers
- **ReflectionEnum** / **ReflectionEnumUnitCase** / **ReflectionEnumBackedCase** - Enum introspection
- **ReflectionAttribute** - Attribute introspection (PHP 8.0+)
- **ReflectionGenerator** / **ReflectionFiber** - Generator and fiber introspection
- **ReflectionExtension** - Extension introspection
- **ReflectionZendExtension** - Zend extension introspection

Additional method implementations needed:
- `ReflectionClass::newInstance()` / `newInstanceArgs()` - Dynamic instantiation
- `ReflectionClass::getMethod()` / `getProperty()` - Return ReflectionMethod/Property objects
- `ReflectionFunction::invoke()` / `invokeArgs()` - Dynamic invocation
- `ReflectionFunction::getParameters()` - Return ReflectionParameter objects
- Static reflection methods (`Reflection::export()`, `getModifierNames()`)

## References

- PHP Manual: https://www.php.net/manual/en/book.reflection.php
- PHP Source: `$PHP_SRC_PATH/ext/reflection/`
- PHP Source: `$PHP_SRC_PATH/Zend/zend_reflection.c`
