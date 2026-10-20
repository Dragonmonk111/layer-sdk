build *args='':
    cargo build $@

check-fmt:
    just fmt --check

fmt *args='':
    cargo +nightly fmt --all $@

clippy *args='':
    cargo clippy --all-targets $@ -- -D warnings

lint: check-fmt clippy

test *args='':
    cargo nextest run $@

test-docs:
    cargo test --doc

check-docs:
    cargo doc --no-deps

bench *args='':
    cargo bench $@

# Run zepter feature checks
check-features:
    zepter run check && zepter format features

# Fix feature propagation and formatting
fix-features:
    zepter && zepter format features --fix
