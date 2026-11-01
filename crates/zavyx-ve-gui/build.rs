// Embeds the Zavyx VE icon and version info into ZavyxVE.exe.
fn main() {
    println!("cargo:rerun-if-changed=../../assets/zavyx-ve.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/zavyx-ve.ico")
            .set("ProductName", "Zavyx Video Editor")
            .set("FileDescription", "Zavyx Video Editor")
            .set("CompanyName", "Mohammad Ibrahim")
            .set("OriginalFilename", "ZavyxVE.exe")
            .set("LegalCopyright", "Copyright (c) 2026 Mohammad Ibrahim - free to use, not for sale");
        res.compile().expect("failed to embed Windows icon resource");
    }
}
