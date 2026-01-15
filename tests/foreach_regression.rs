mod common;

use common::run_code_capture_output;

#[test]
fn test_foreach_over_array_after_is_array_guard() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        $required_php_extensions = array('json', 'hash');
        if (isset($required_php_extensions) && is_array($required_php_extensions)) {
            foreach ($required_php_extensions as $extension) {
                echo $extension;
            }
        }
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "jsonhash");
}

#[test]
fn test_foreach_over_global_array_in_function() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        $required_php_extensions = array('json', 'hash');
        function check_exts() {
            global $required_php_extensions;
            if (isset($required_php_extensions) && is_array($required_php_extensions)) {
                foreach ($required_php_extensions as $extension) {
                    echo $extension;
                }
            }
        }
        check_exts();
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "jsonhash");
}

#[test]
fn test_foreach_over_default_array_property() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        class Hook {
            public $callbacks = array();
            public function run() {
                foreach ($this->callbacks as $cb) {
                    echo $cb;
                }
            }
        }
        $hook = new Hook();
        $hook->run();
        echo "ok";
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "ok");
}

#[test]
fn test_foreach_over_associative_array() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        $tables = array('users' => 'wp_users', 'posts' => 'wp_posts');
        foreach ($tables as $key => $value) {
            echo $key . ':' . $value . ';';
        }
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "users:wp_users;posts:wp_posts;");
}

#[test]
fn test_foreach_assigns_dynamic_properties() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        #[AllowDynamicProperties]
        class T {}
        $t = new T();
        $arr = array('a' => 'x', 'b' => 'y');
        foreach ($arr as $k => $v) {
            $t->$k = $v;
        }
        echo $t->a . $t->b;
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "xy");
}

#[test]
fn test_set_magic_allows_setting_dynamic_property() {
    let (_, output) = run_code_capture_output(
        r#"<?php
        #[AllowDynamicProperties]
        class T {
            public function __set($name, $value) {
                $this->$name = $value;
            }
        }
        $t = new T();
        $name = 'bar';
        $t->$name = 'x';
        echo $t->bar;
        "#,
    )
    .expect("execution failed");

    assert_eq!(output, "x");
}
