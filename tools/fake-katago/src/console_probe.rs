#[allow(unsafe_code)]
pub fn window() -> &'static str {
    // SAFETY: GetConsoleWindow takes no pointers or arguments. We only compare
    // the returned process-owned handle with null; we never dereference it.
    let window = unsafe { windows_sys::Win32::System::Console::GetConsoleWindow() };
    if window.is_null() {
        "console:none"
    } else {
        "console:present"
    }
}
