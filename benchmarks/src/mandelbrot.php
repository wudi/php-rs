<?php
$start = microtime(true);

function mandelbrot($w, $h) {
    $out = '';
    for ($y = 0; $y < $h; $y++) {
        for ($x = 0; $x < $w; $x++) {
            $zr = 0;
            $zi = 0;
            $cr = ($x / $w) * 2.5 - 1.5; // Real part
            $ci = ($y / $h) * 2.0 - 1.0; // Imaginary part
            $i = 0;

            while ($zr * $zr + $zi * $zi < 4 && $i < 50) {
                $temp = $zr * $zr - $zi * $zi + $cr;
                $zi = 2 * $zr * $zi + $ci;
                $zr = $temp;
                $i++;
            }
            // Just output a character based on iterations
            $out .= ($i == 50) ? ' ' : '*';
        }
        $out .= "\n";
    }
    return $out;
}

// Low resolution for speed in benchmark
$result = mandelbrot(60, 30);
echo "Mandelbrot generated. Length: " . strlen($result) . "\n";
echo "Time: " . (microtime(true) - $start) . "s";

