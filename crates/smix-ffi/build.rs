fn main() {
    uniffi::generate_scaffolding("./src/smix.udl").expect("UniFFI scaffolding generation failed");
}
