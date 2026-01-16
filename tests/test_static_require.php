<?php
function test_static_require() {
    static $data;
    if (!$data) {
        echo "Loading...\n";
        $data = require __DIR__ . '/return_array.php';
    } else {
        echo "Cached.\n";
    }
    var_dump($data);
}

test_static_require();
test_static_require();

