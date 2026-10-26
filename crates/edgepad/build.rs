//! Embeds the manifest that asks for the common controls version 6 (the
//! settings window's buttons, lists and fields in Windows's own look).
//!
//! It goes in as resource 2, not 1: Windows applies resource 1 to the whole
//! process at its start, which would load comctl32 into every EdgePad,
//! settings opened or not. The settings window applies resource 2 itself,
//! when it opens (spec 7.1, 8).

fn main() {
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED,ID=2");
        println!("cargo:rustc-link-arg-bins=/MANIFESTUAC:NO");
        println!(
            "cargo:rustc-link-arg-bins=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' \
             version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
    }
}
