fn main() {
    // Library test binaries also link native dialogs, whose TaskDialogIndirect
    // import requires Common Controls v6 before the Rust test harness can start.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("windows-common-controls.manifest");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        // Tauri already embeds resource 1 in application binaries. Keep that
        // manifest authoritative while library test executables receive ID 1.
        println!("cargo:rustc-link-arg-bin=runory=/MANIFEST:EMBED,ID=2");
        println!("cargo:rustc-link-arg-bin=runory-plugin=/MANIFEST:EMBED,ID=2");
    }
    println!("cargo:rerun-if-env-changed=RUNORY_POLICY_VERIFYING_KEY_BASE64");
    println!("cargo:rerun-if-env-changed=RUNORY_POLICY_VERIFYING_KEYS_JSON");
    tauri_build::build()
}
