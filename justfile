firmware-build:
    cargo build --target thumbv8m.main-none-eabihf -p ir_sunrise

firmware-check:
    cargo check --target thumbv8m.main-none-eabihf -p ir_sunrise

firmware-flash:
    cargo run --target thumbv8m.main-none-eabihf -p ir_sunrise --bin ir-sunrise

server-build:
    cargo build -p sunrise-server

server-check:
    cargo check -p sunrise-server

server-run:
    cargo run -p sunrise-server

server-docker:
    docker buildx build \
        --platform linux/amd64,linux/arm64 \
        --tag orndorffgrant/sunrise-server:latest \
        --file sunrise-server/Dockerfile \
        .

server-docker-push:
    docker buildx build \
        --platform linux/amd64,linux/arm64 \
        --tag orndorffgrant/sunrise-server:latest \
        --push \
        --file sunrise-server/Dockerfile \
        .

build: firmware-build server-build

check: firmware-check server-check

enclosure-build: enclosure-sync
    cd enclosure && uv run python box.py

enclosure-sync:
    cd enclosure && uv sync

board-build: board-sync
    cd board && uv run python board.py

board-sync:
    cd board && uv sync

clean:
    cargo clean
