mod common;

use common::run_code_capture_output;

#[test]
fn test_reflection_class_get_attributes_basic() {
    let code = r#"<?php
class Example {}
#[Example(1, name: "x")]
class Foo {}
$ref = new ReflectionClass(Foo::class);
$attrs = $ref->getAttributes();
echo count($attrs), "\n";
echo $attrs[0]->getName(), "\n";
var_dump($attrs[0]->getArguments());
"#;
    let (_, output) = run_code_capture_output(code).unwrap();
    assert!(output.contains("1\n"));
    assert!(output.contains("Example\n"));
    assert!(output.contains("array(2)"));
}

#[test]
#[should_panic]
fn test_attribute_disallows_unpacking() {
    let code = r#"<?php
#[Example(...[1,2])]
class Foo {}
"#;
    let _ = common::run_code(code);
}

#[test]
#[should_panic]
fn test_attribute_disallows_closure_argument() {
    let code = r#"<?php
#[Example(fn() => 1)]
class Foo {}
"#;
    let _ = common::run_code(code);
}

#[test]
#[should_panic]
fn test_attribute_disallows_positional_after_named() {
    let code = r#"<?php
#[Example(name: "x", 1)]
class Foo {}
"#;
    let _ = common::run_code(code);
}

#[test]
#[should_panic]
fn test_attribute_disallows_duplicate_named() {
    let code = r#"<?php
#[Example(name: "x", name: "y")]
class Foo {}
"#;
    let _ = common::run_code(code);
}

#[test]
fn test_reflection_function_method_property_param_const_attributes() {
    let code = r#"<?php
#[Example]
function foo(#[ExampleParam] $x) {}
class Foo {
    #[ExampleProp]
    public int $x;
    #[ExampleConst]
    public const C = 1;
    #[ExampleMethod]
    public function bar(#[ExampleParam] $y) {}
}
$rf = new ReflectionFunction('foo');
$rm = new ReflectionMethod(Foo::class, 'bar');
$rp = new ReflectionProperty(Foo::class, 'x');
$rc = new ReflectionClassConstant(Foo::class, 'C');
$attrs = [$rf->getAttributes(), $rm->getAttributes(), $rp->getAttributes(), $rc->getAttributes()];
foreach ($attrs as $list) { echo count($list), "\n"; }
"#;
    let (_, output) = run_code_capture_output(code).unwrap();
    assert!(output.contains("1\n1\n1\n1\n"));
}

#[test]
fn test_attribute_target_validation_on_new_instance() {
    let code = r#"<?php
#[Attribute(Attribute::TARGET_PROPERTY)]
class OnlyProp {}
#[OnlyProp]
class Foo {}
$attrs = (new ReflectionClass(Foo::class))->getAttributes();
$attrs[0]->newInstance();
"#;
    let err = run_code_capture_output(code).unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("cannot target"));
}

#[test]
fn test_attribute_repeatable_validation_on_new_instance() {
    let code = r#"<?php
#[Attribute]
class NonRepeat {}
#[NonRepeat]
#[NonRepeat]
class Foo {}
$attrs = (new ReflectionClass(Foo::class))->getAttributes();
$attrs[0]->newInstance();
"#;
    let err = run_code_capture_output(code).unwrap_err();
    let msg = format!("{err:?}");
    assert!(msg.contains("must not be repeated"));
}
