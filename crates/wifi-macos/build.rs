//! Embeds an Info.plist in the `probe` example, so macOS shows it the Location
//! prompt (a command-line tool without one is never asked, and never sees
//! Wi-Fi network names).

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo");
    let plist = format!("{dir}/examples/probe-Info.plist");
    println!("cargo:rerun-if-changed={plist}");
    println!("cargo:rustc-link-arg-examples=-Wl,-sectcreate,__TEXT,__info_plist,{plist}");
}
