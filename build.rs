fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("build/icon.ico");
        res.compile().expect("compile windows resources");
    }
}
