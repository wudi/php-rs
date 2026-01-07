<?php
function fib($n) {
    if ($n <= 1) return $n;
    return fib($n - 1) + fib($n - 2);
}

// Calculate 20th Fibonacci number
echo "Fibonacci(20): " . fib(20);
