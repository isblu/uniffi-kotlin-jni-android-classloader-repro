fn main() {
    // Generates $OUT_DIR/uniffi_bindgen_kotlin_jni.uniffi.rs (JNI scaffolding), included by src/lib.rs.
    uniffi_bindgen_kotlin_jni::generate_scaffolding();
}
