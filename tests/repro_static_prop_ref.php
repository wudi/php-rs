<?php
class RegistryRef {
    public static $collections = [
        'path' => ['metadata' => null, 'manifest' => 'return_array.php']
    ];

    public static function load($path) {
        $collection = &self::$collections[$path];
        if (null === $collection['metadata']) {
             echo "Loading...\n";
             $collection['metadata'] = require __DIR__ . '/return_array.php';
        } else {
             echo "Cached.\n";
        }
    }
}
RegistryRef::load('path');
RegistryRef::load('path');

