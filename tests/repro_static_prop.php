<?php
class Registry {
    public static $data = [];
    public static function load() {
        if (!isset(self::$data['key'])) {
            echo "Loading...
";
            self::$data['key'] = require __DIR__ . '/return_array.php';
        } else {
            echo "Cached.
";
        }
    }
}
Registry::load();
Registry::load();

