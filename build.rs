fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        if std::path::Path::new("assets/nocturne.ico").exists() {
            res.set_icon("assets/nocturne.ico");
        }
        let _ = res.compile();
    }
}