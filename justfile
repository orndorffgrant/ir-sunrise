build-fw:
    cargo build --target thumbv8m.main-none-eabihf -p ir_sunrise

build-server:
    cargo build -p sunrise-server

build:
    cargo build --target thumbv8m.main-none-eabihf -p ir_sunrise
    cargo build -p sunrise-server

check:
    cargo check --target thumbv8m.main-none-eabihf -p ir_sunrise
    cargo check -p sunrise-server

flash:
    cargo run --target thumbv8m.main-none-eabihf -p ir_sunrise --bin ir-sunrise

run-server:
    cargo run -p sunrise-server

clean:
    cargo clean
