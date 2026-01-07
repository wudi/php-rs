<?php
$start = microtime(true);

$base = "The quick brown fox jumps over the lazy dog. ";
$large_string = str_repeat($base, 1000); // ~45KB string

for ($i = 0; $i < 100; $i++) {
    // Replace
    $s = str_replace("fox", "cat", $large_string);
    $s = str_replace("dog", "bird", $s);
    
    // Substring
    $sub = substr($s, 0, 500);
    
    // Concatenation
    $s = $sub . " -- suffix";
    
    // Search
    $pos = strpos($s, "bird");
}

echo "String ops completed.\n";
echo "Time: " . (microtime(true) - $start) . "s";

