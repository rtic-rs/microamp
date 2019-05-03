set -euxo pipefail

main() {
    ( cd tools && cargo install --path . --debug -f )

    if [ $TARGET = x86_64-unknown-linux-gnu ]; then
        cargo check
    else
        ( cd dummy && cargo microamp --bin dummy --check )
    fi
}

# fake Travis variables to be able to run this on a local machine
if [ -z ${TARGET-} ]; then
    TARGET=$(rustc -Vv | grep host | cut -d ' ' -f2)
fi

main
