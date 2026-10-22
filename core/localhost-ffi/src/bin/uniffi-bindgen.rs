//! Generates the Swift bindings from the built library (`scripts/build-core.sh`).

fn main() {
    uniffi::uniffi_bindgen_main()
}
