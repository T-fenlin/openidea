//! Manual Windows check: a peer spawned by [`Peer::spawn`] must not get a console window.
//!
//! Run it from a shell that has a console; it detaches from that console first, so
//! it behaves like the release IDE (a GUI-subsystem process with no console) and
//! then compares a plain spawn with what [`Peer::spawn`] does:
//!
//! ```text
//! cargo run -p ide-jsonrpc --example console_check
//! ```
//!
//! The result is appended to `target/console-probe/peer-check.log`.

#[cfg(windows)]
mod check {
    use ide_jsonrpc::{NoopHandler, Peer};
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    const LOG: &str = "target/console-probe/peer-check.log";

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
        fn FreeConsole() -> i32;
        fn GetConsoleWindow() -> *mut core::ffi::c_void;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn IsWindowVisible(hwnd: *mut core::ffi::c_void) -> i32;
    }

    /// Attaches to `pid`'s console and reports whether that console owns a window.
    fn console_window(pid: u32) -> String {
        std::thread::sleep(Duration::from_millis(400));
        // SAFETY: plain Win32 console calls; every handle used comes back from the API.
        unsafe {
            if AttachConsole(pid) == 0 {
                return format!("no console to attach to: {}", std::io::Error::last_os_error());
            }
            let hwnd = GetConsoleWindow();
            let visible = if hwnd.is_null() { -1 } else { IsWindowVisible(hwnd) };
            FreeConsole();
            if hwnd.is_null() {
                "console without a window".into()
            } else {
                format!("console window {hwnd:p}, visible={visible}")
            }
        }
    }

    fn log(line: &str) {
        let _ = std::fs::create_dir_all("target/console-probe");
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(LOG) {
            let _ = writeln!(f, "{line}");
        }
    }

    fn child() -> (PathBuf, Vec<String>) {
        let exe = std::env::var_os("SystemRoot")
            .map(|root| PathBuf::from(root).join("System32").join("PING.EXE"))
            .unwrap_or_else(|| PathBuf::from("ping"));
        (exe, vec!["-n".to_owned(), "5".to_owned(), "127.0.0.1".to_owned()])
    }

    pub fn run() {
        // SAFETY: the harness starts attached to the caller's console; detaching is safe.
        unsafe {
            FreeConsole();
        }

        let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
        let (exe, args) = child();

        runtime.block_on(async {
            // Control: a plain spawn, which is what `Peer::spawn` used to do.
            let mut control = tokio::process::Command::new(&exe);
            control.args(&args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null());
            let control = control.spawn().expect("spawn control");
            if let Some(pid) = control.id() {
                log(&format!("plain tokio spawn : {}", console_window(pid)));
            }

            // The real code path used for language servers and debug adapters.
            let peer = Peer::spawn(
                &exe.to_string_lossy(),
                &args,
                &std::env::current_dir().expect("cwd"),
                &[],
                Arc::new(NoopHandler),
                Box::new(|_| {}),
            )
            .expect("spawn peer");
            if let Some(pid) = peer.pid() {
                log(&format!("Peer::spawn       : {}", console_window(pid)));
            }
            peer.kill();
        });
    }
}

fn main() {
    #[cfg(windows)]
    check::run();

    #[cfg(not(windows))]
    println!("console_check inspects Windows console windows only.");
}
