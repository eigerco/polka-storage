fn main() {
    // This ensures that the library recompiles when the SCALE file suffers changes
    // To generate a new scale file use `just generate-scale` after running the parachain.
    println!("cargo::rerun-if-changed=../../artifacts/metadata.scale")
}
