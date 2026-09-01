fn main() {
    println!("cargo:rerun-if-env-changed=RUNORY_POLICY_VERIFYING_KEY_BASE64");
    println!("cargo:rerun-if-env-changed=RUNORY_POLICY_VERIFYING_KEYS_JSON");
    tauri_build::build()
}
