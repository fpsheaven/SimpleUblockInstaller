fn main() {
    // Embed the FPSHEAVEN logo as the .exe file icon (shown in Explorer/taskbar).
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        if let Err(err) = res.compile() {
            println!("cargo:warning=could not embed the exe icon: {err}");
        }
    }
}
