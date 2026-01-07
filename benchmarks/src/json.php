<?php
// Generate a large array and serialize it to test memory and string manipulation
$data = [];
for ($i = 0; $i < 1000; $i++) {
    $data[] = [
        'id' => $i,
        'name' => "Item $i",
        'values' => [$i, $i+1, $i+2],
        'meta' => str_repeat("x", 50)
    ];
}

$json = json_encode($data);

echo "Processed items. JSON size: " . strlen($json) . " bytes.";
