fn main() {
    // This ensures that the library recompiles when the SCALE file suffers changes
    println!("cargo::rerun-if-changed=../../artifacts/metadata.scale")
}
