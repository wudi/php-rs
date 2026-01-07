#!/bin/bash
set -e

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

# Check if wrk is installed
if ! command -v wrk &> /dev/null;
then
    echo "Error: 'wrk' is not installed. Please install it (e.g., sudo apt install wrk) to run benchmarks."
    exit 1
fi

echo -e "${BLUE}Starting Docker environment...${NC}"
docker compose up -d --build

echo -e "${BLUE}Waiting for services to be ready...${NC}"
sleep 5

run_bench() {
    local name=$1
    local port=$2
    local script=$3
    
    echo -e "\n${GREEN}=== Benchmarking $name - $script ===${NC}"
    echo "URL: http://localhost:$port/$script"
    wrk -t4 -c100 -d10s "http://localhost:$port/$script"
}

# Run benchmarks
echo -e "\n${BLUE}Starting Benchmarks...${NC}"

# Hello World
run_bench "Native PHP" 8001 "hello.php"
run_bench "Rust PHP"   8002 "hello.php"

# Fibonacci
run_bench "Native PHP" 8001 "fib.php"
run_bench "Rust PHP"   8002 "fib.php"

# JSON
run_bench "Native PHP" 8001 "json.php"
run_bench "Rust PHP"   8002 "json.php"

# Mandelbrot
run_bench "Native PHP" 8001 "mandelbrot.php"
run_bench "Rust PHP"   8002 "mandelbrot.php"

# Objects
run_bench "Native PHP" 8001 "objects.php"
run_bench "Rust PHP"   8002 "objects.php"

# Strings
run_bench "Native PHP" 8001 "strings.php"
run_bench "Rust PHP"   8002 "strings.php"

echo -e "\n${BLUE}Cleaning up...${NC}"
# docker compose down
echo "Done. Run 'cd benchmarks && docker compose down' to stop containers."
