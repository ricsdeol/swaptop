use super::PlatformProvider;

pub fn detect() -> Box<dyn PlatformProvider> {
    #[cfg(target_os = "linux")]
    return Box::new(super::linux::LinuxBackend::new());

    #[cfg(target_os = "macos")]
    return Box::new(super::macos::MacosBackend::new());

    #[cfg(target_os = "windows")]
    return Box::new(super::windows::WindowsBackend::new());

    #[cfg(any(target_os = "freebsd", target_os = "openbsd"))]
    return Box::new(super::bsd::BsdBackend::new());

    #[cfg(not(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "windows",
        target_os = "freebsd",
        target_os = "openbsd"
    )))]
    compile_error!("unsupported platform");
}
