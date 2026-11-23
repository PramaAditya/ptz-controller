fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "PTZ Controller");
        res.set(
            "FileDescription",
            "Native Windows remote for USB UVC PTZ cameras",
        );
        res.set("LegalCopyright", "MIT License");
        res.compile().expect("failed to compile Windows icon resource");
    }
}
