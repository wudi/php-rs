mod common;

use common::run_code_with_vm;
use php_rs::core::value::{ArrayKey, Val};
use std::collections::HashMap;

#[test]
fn test_reflection_class_get_trait_aliases() {
    let (result, vm) = run_code_with_vm(r#"<?php
        trait TA {
            public function foo() {}
        }

        trait TB {
            public function bar() {}
        }

        class AliasUser {
            use TA, TB {
                TA::foo as fooAlias;
                bar as baz;
                TB::bar as private barAlias;
                TB::bar as private;
            }
        }

        return (new ReflectionClass('AliasUser'))->getTraitAliases();
    "#)
    .expect("execution failed");

    let Val::Array(arr) = result else { panic!("expected array"); };
    let mut aliases = HashMap::new();
    for (key, handle) in &arr.map {
        let ArrayKey::Str(key) = key else { continue };
        let Val::String(value) = vm.arena.get(*handle).value.clone() else { continue };
        aliases.insert(key.as_ref().to_vec(), value.as_ref().to_vec());
    }

    assert_eq!(
        aliases.get(b"fooAlias".as_slice()),
        Some(&b"TA::foo".to_vec())
    );
    assert_eq!(aliases.get(b"baz".as_slice()), Some(&b"TB::bar".to_vec()));
    assert_eq!(
        aliases.get(b"barAlias".as_slice()),
        Some(&b"TB::bar".to_vec())
    );
    assert_eq!(aliases.len(), 3);
}
