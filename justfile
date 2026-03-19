backend_dir := "backend"
frontend_dir := "frontend"
music_dir := "/tmp/music"

# Run both frontend and backend for local development
dev:
    #!/usr/bin/env bash
    set -euo pipefail
    set -m  # enable job control so children get their own process groups

    pids=()
    cleanup() {
        echo "Shutting down..."
        for pid in "${pids[@]}"; do
            kill -- -"$pid" 2>/dev/null || true
        done
        wait 2>/dev/null
    }
    trap cleanup EXIT INT TERM

    mkdir -p {{music_dir}}
    export MUSIC_DIR={{music_dir}}

    (cd {{backend_dir}} && exec cargo run) &
    pids+=($!)
    (cd {{frontend_dir}} && exec npm run dev) &
    pids+=($!)

    wait

# Run only the backend
dev-backend:
    mkdir -p {{music_dir}}
    MUSIC_DIR={{music_dir}} cargo run --manifest-path {{backend_dir}}/Cargo.toml

# Run only the frontend
dev-frontend:
    cd {{frontend_dir}} && npm run dev

# Check backend compiles
check:
    cd {{backend_dir}} && cargo check

# Build backend in release mode
build:
    cd {{backend_dir}} && cargo build --release
