// Mechanism probe: does a console-subsystem child spawned from a *console-less*
// (GUI-subsystem) parent get a visible console window, and does CREATE_NO_WINDOW
// change that? Build and run it by hand; it appends its findings to
// `target/console-probe/probe.log` (see README.md in this directory).
//
//     rustc mechanism-probe.rs -O -o mechanism-probe.exe && ./mechanism-probe.exe
#![windows_subsystem = "windows"]

use std::io::Write;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const LOG: &str = "target/console-probe/probe.log";

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

/// Attaches to `child`'s console and reports whether that console owns a window.
fn console_window(child: &mut Child) -> String {
    let alive = matches!(child.try_wait(), Ok(None));
    let pid = child.id();
    // SAFETY: plain Win32 console calls; the hWnd comes back from the API.
    unsafe {
        if AttachConsole(pid) == 0 {
            return format!("AttachConsole failed (alive={alive}): {}", std::io::Error::last_os_error());
        }
        let hwnd = GetConsoleWindow();
        let visible = if hwnd.is_null() { -1 } else { IsWindowVisible(hwnd) };
        FreeConsole();
        if hwnd.is_null() {
            "no console window (GetConsoleWindow = NULL)".into()
        } else {
            format!("console window {hwnd:p}, visible={visible}")
        }
    }
}

fn spawn(cmd: &mut Command) -> Child {
    cmd.args(["-n", "5", "127.0.0.1"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().expect("spawn PING.EXE")
}

/// `PING.EXE` from the Windows directory, so the probe works from any checkout.
fn ping() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(|root| PathBuf::from(root).join("System32").join("PING.EXE"))
        .unwrap_or_else(|| PathBuf::from("ping"))
}

fn main() {
    // Start console-less, exactly like the release IDE (windows_subsystem = "windows").
    // SAFETY: the probe starts attached to the caller's console; detaching is safe.
    unsafe {
        FreeConsole();
    }

    let ping = ping();
    let mut out = String::new();
    let mut naive = spawn(&mut Command::new(&ping));
    std::thread::sleep(std::time::Duration::from_millis(600));
    out.push_str(&format!("plain spawn           : {}\n", console_window(&mut naive)));

    let mut hidden_cmd = Command::new(&ping);
    hidden_cmd.creation_flags(CREATE_NO_WINDOW);
    let mut hidden = spawn(&mut hidden_cmd);
    std::thread::sleep(std::time::Duration::from_millis(600));
    out.push_str(&format!("with CREATE_NO_WINDOW : {}\n", console_window(&mut hidden)));

    let _ = std::fs::create_dir_all("target/console-probe");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(LOG) {
        let _ = write!(f, "{out}");
    }
    for child in [&mut naive, &mut hidden] {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
}
