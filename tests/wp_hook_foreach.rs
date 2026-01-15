mod common;

use common::run_code;
use php_rs::core::value::Val;

#[test]
fn test_wp_hook_style_callbacks_iteration() {
    let code = r#"<?php
class MiniHook {
    public $callbacks = array();
    protected $priorities = array();
    private $iterations = array();
    private $current_priority = array();
    private $nesting_level = 0;

    public function add($priority, $idx) {
        $this->callbacks[$priority][$idx] = array(
            'function' => $idx,
            'accepted_args' => 1,
        );
        $this->priorities = array_keys($this->callbacks);
    }

    public function run() {
        $nesting_level = $this->nesting_level++;
        $this->iterations[$nesting_level] = $this->priorities;
        $this->current_priority[$nesting_level] = current($this->iterations[$nesting_level]);
        $priority = $this->current_priority[$nesting_level];
        foreach ($this->callbacks[$priority] as $the_) {
            // no-op
        }
        return $priority;
    }
}

$hook = new MiniHook();
$hook->add(10, 'cb');
return $hook->run();
"#;

    let val = run_code(code);
    assert_eq!(val, Val::Int(10));
}
