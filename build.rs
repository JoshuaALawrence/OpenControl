fn main() {
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rerun-if-changed=assets/icon.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "OpenControl");
        res.set(
            "FileDescription",
            "A full Windows computer control over the Model Context Protocol",
        );
        res.set("CompanyName", "OpenControl");
        res.set("OriginalFilename", "OpenControl.exe");
        res.set("InternalName", "OpenControl");
        res.set(
            "LegalCopyright",
            "Copyright 2026 Joshua Lawrence — GNU AGPL v3.0 or later",
        );
        if let Err(e) = res.compile() {
            println!("cargo:warning=failed to embed Windows resources: {e}");
        }
    }
}
