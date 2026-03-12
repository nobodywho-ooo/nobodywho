fn main() {
    #[cfg(feature = "uniffi")]
    uniffi::generate_scaffolding("src/nobodywho.udl").unwrap();
}
