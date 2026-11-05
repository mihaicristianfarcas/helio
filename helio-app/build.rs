//! On Windows: the icon and version information in Helio.exe itself.

fn main() {
    println!("cargo:rerun-if-changed=assets/Helio.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/Helio.ico");
    res.set("ProductName", "Helio");
    res.set("FileDescription", "Helio");
    if let Err(e) = res.compile() {
        println!("cargo:warning=Helio.exe without its icon: {e}");
    }
}
