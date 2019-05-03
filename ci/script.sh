set -euxo pipefail

main() {
    cargo check

    ( cd tools && cargo install --path . --debug -f )
}

main
