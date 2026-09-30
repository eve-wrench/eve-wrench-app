fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("../../assets/icons/icon.ico");
        resource
            .compile()
            .expect("failed to embed the Windows icon");
    }
}
