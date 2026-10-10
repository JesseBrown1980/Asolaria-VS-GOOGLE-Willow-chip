//! Windows pseudo-console (ConPTY) through kernel32, no crates.
//!
//! A shell started here gets a real console: prompts, colours, cursor addressing, full-screen
//! programs (vim, htop over WSL, the claude / codex TUIs) and window resizes all work, because
//! conhost renders the console as VT sequences onto our output pipe and parses VT keystrokes from
//! our input pipe. The pipe path in `term.rs` stays as the fallback when this fails.
//!
//! Lifetime: a waiter thread blocks on the process handle; when the process exits it closes the
//! pseudo-console, which ends the output pipe, which ends the reader — so a shell that exits on
//! its own is seen as ended. `kill` terminates the process and the same waiter does the close.

use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::os::windows::io::FromRawHandle;
use std::sync::{Arc, Mutex};

type Handle = *mut c_void;

const EXTENDED_STARTUPINFO_PRESENT: u32 = 0x0008_0000;
const CREATE_UNICODE_ENVIRONMENT: u32 = 0x0000_0400;
const STARTF_USESTDHANDLES: u32 = 0x0000_0100;
const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x0002_0016;
const INFINITE: u32 = 0xFFFF_FFFF;
/// INVALID_HANDLE_VALUE: the child must take its std handles from the pseudo-console, not
/// inherit whatever this server's own stdio happens to be.
const INVALID: Handle = usize::MAX as Handle;

#[repr(C)]
#[derive(Clone, Copy)]
struct Coord {
    x: i16,
    y: i16,
}

#[repr(C)]
struct StartupInfoW {
    cb: u32,
    reserved: *mut u16,
    desktop: *mut u16,
    title: *mut u16,
    x: u32,
    y: u32,
    x_size: u32,
    y_size: u32,
    x_count_chars: u32,
    y_count_chars: u32,
    fill_attribute: u32,
    flags: u32,
    show_window: u16,
    cb_reserved2: u16,
    reserved2: *mut u8,
    std_input: Handle,
    std_output: Handle,
    std_error: Handle,
}

#[repr(C)]
struct StartupInfoExW {
    startup_info: StartupInfoW,
    attribute_list: *mut c_void,
}

#[repr(C)]
struct ProcessInformation {
    process: Handle,
    thread: Handle,
    process_id: u32,
    thread_id: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn CreatePipe(read: *mut Handle, write: *mut Handle, attrs: *const c_void, size: u32) -> i32;
    fn CreatePseudoConsole(
        size: Coord,
        input: Handle,
        output: Handle,
        flags: u32,
        pc: *mut Handle,
    ) -> i32;
    fn ResizePseudoConsole(pc: Handle, size: Coord) -> i32;
    fn ClosePseudoConsole(pc: Handle);
    fn InitializeProcThreadAttributeList(
        list: *mut c_void,
        count: u32,
        flags: u32,
        size: *mut usize,
    ) -> i32;
    fn UpdateProcThreadAttribute(
        list: *mut c_void,
        flags: u32,
        attribute: usize,
        value: *const c_void,
        size: usize,
        previous: *mut c_void,
        return_size: *mut usize,
    ) -> i32;
    fn DeleteProcThreadAttributeList(list: *mut c_void);
    fn CreateProcessW(
        application: *const u16,
        command_line: *mut u16,
        process_attrs: *const c_void,
        thread_attrs: *const c_void,
        inherit_handles: i32,
        flags: u32,
        environment: *const c_void,
        current_directory: *const u16,
        startup_info: *const StartupInfoExW,
        process_information: *mut ProcessInformation,
    ) -> i32;
    fn WaitForSingleObject(handle: Handle, millis: u32) -> u32;
    fn TerminateProcess(handle: Handle, code: u32) -> i32;
    fn CloseHandle(handle: Handle) -> i32;
    fn SetConsoleCtrlHandler(handler: *const c_void, add: i32) -> i32;
}

struct State {
    pc: usize,
    process: usize,
    closed: bool,
}

/// A live pseudo-console session. Handles are stored as integers so the struct is `Send`;
/// every use goes through the mutex and checks `closed` first, so a handle is never used
/// after it was released.
pub struct Pty {
    st: Arc<Mutex<State>>,
}

impl Pty {
    pub fn resize(&self, cols: i16, rows: i16) -> bool {
        let st = self.st.lock().unwrap();
        if st.closed {
            return false;
        }
        unsafe { ResizePseudoConsole(st.pc as Handle, Coord { x: cols, y: rows }) == 0 }
    }

    pub fn kill(&self) {
        let st = self.st.lock().unwrap();
        if !st.closed {
            unsafe {
                TerminateProcess(st.process as Handle, 1);
            }
        }
    }
}

fn close(st: &Mutex<State>) {
    let mut s = st.lock().unwrap();
    if !s.closed {
        s.closed = true;
        unsafe {
            ClosePseudoConsole(s.pc as Handle);
            CloseHandle(s.process as Handle);
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn last_err(what: &str) -> io::Error {
    let e = io::Error::last_os_error();
    io::Error::new(e.kind(), format!("{what}: {e}"))
}

/// Start `command_line` attached to a new pseudo-console of `cols`×`rows`.
/// Returns the session, the output reader, and the input writer.
pub fn spawn(
    command_line: &str,
    cwd: Option<&str>,
    cols: i16,
    rows: i16,
) -> io::Result<(Pty, File, File)> {
    unsafe {
        let (mut in_r, mut in_w, mut out_r, mut out_w): (Handle, Handle, Handle, Handle) = (
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        if CreatePipe(&mut in_r, &mut in_w, std::ptr::null(), 0) == 0 {
            return Err(last_err("CreatePipe(in)"));
        }
        if CreatePipe(&mut out_r, &mut out_w, std::ptr::null(), 0) == 0 {
            CloseHandle(in_r);
            CloseHandle(in_w);
            return Err(last_err("CreatePipe(out)"));
        }
        let mut pc: Handle = std::ptr::null_mut();
        let hr = CreatePseudoConsole(Coord { x: cols, y: rows }, in_r, out_w, 0, &mut pc);
        // conhost holds its own duplicates of the far ends now.
        CloseHandle(in_r);
        CloseHandle(out_w);
        if hr != 0 {
            CloseHandle(in_w);
            CloseHandle(out_r);
            return Err(io::Error::other(format!(
                "CreatePseudoConsole hresult=0x{:08x}",
                hr as u32
            )));
        }
        let reader = File::from_raw_handle(out_r);
        let writer = File::from_raw_handle(in_w);

        let mut size = 0usize;
        InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size);
        // usize-aligned backing store for the opaque attribute list
        let mut buf = vec![0usize; size / std::mem::size_of::<usize>() + 1];
        let list = buf.as_mut_ptr() as *mut c_void;
        if InitializeProcThreadAttributeList(list, 1, 0, &mut size) == 0 {
            ClosePseudoConsole(pc);
            return Err(last_err("InitializeProcThreadAttributeList"));
        }
        if UpdateProcThreadAttribute(
            list,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
            pc as *const c_void,
            std::mem::size_of::<Handle>(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ) == 0
        {
            DeleteProcThreadAttributeList(list);
            ClosePseudoConsole(pc);
            return Err(last_err("UpdateProcThreadAttribute"));
        }

        let mut si: StartupInfoExW = std::mem::zeroed();
        si.startup_info.cb = std::mem::size_of::<StartupInfoExW>() as u32;
        si.startup_info.flags = STARTF_USESTDHANDLES;
        si.startup_info.std_input = INVALID;
        si.startup_info.std_output = INVALID;
        si.startup_info.std_error = INVALID;
        si.attribute_list = list;
        // Ctrl+C handling is inherited: if whatever launched this server started it with Ctrl+C
        // ignored (a new process group, a service host, a launcher), every shell below would
        // ignore the interrupt conhost raises for ETX (Ctrl+C). Re-enable it for this process first.
        SetConsoleCtrlHandler(std::ptr::null(), 0);
        let mut pi: ProcessInformation = std::mem::zeroed();
        let mut cmd = wide(command_line);
        let dir = cwd.map(wide);
        let ok = CreateProcessW(
            std::ptr::null(),
            cmd.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            std::ptr::null(),
            dir.as_ref().map_or(std::ptr::null(), |d| d.as_ptr()),
            &si,
            &mut pi,
        );
        DeleteProcThreadAttributeList(list);
        drop(buf);
        if ok == 0 {
            let e = last_err("CreateProcessW");
            ClosePseudoConsole(pc);
            return Err(e);
        }
        CloseHandle(pi.thread);

        let st = Arc::new(Mutex::new(State {
            pc: pc as usize,
            process: pi.process as usize,
            closed: false,
        }));
        let waiter = st.clone();
        let process = pi.process as usize;
        std::thread::spawn(move || {
            WaitForSingleObject(process as Handle, INFINITE);
            close(&waiter);
        });
        Ok((Pty { st }, reader, writer))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::time::{Duration, Instant};

    #[test]
    fn a_shell_under_conpty_echoes_vt_and_ends_when_it_exits() {
        let (pty, mut reader, mut writer) =
            spawn("cmd.exe /d /q /k echo ASIOS-PTY-READY", None, 100, 30).expect("conpty");
        let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut b = [0u8; 4096];
            loop {
                match reader.read(&mut b) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let _ = tx.send(b[..n].to_vec());
                    }
                }
            }
        });
        let mut seen = Vec::new();
        let t0 = Instant::now();
        let mut asked = false;
        while t0.elapsed() < Duration::from_secs(20) {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(c) => seen.extend_from_slice(&c),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(_) => {}
            }
            let s = String::from_utf8_lossy(&seen);
            if s.contains("ASIOS-PTY-READY") && !asked {
                assert!(pty.resize(120, 40));
                writer.write_all(b"exit\r").unwrap();
                asked = true;
            }
        }
        let s = String::from_utf8_lossy(&seen);
        assert!(s.contains("ASIOS-PTY-READY"), "no marker in: {s:?}");
        // conhost speaks VT on the output pipe
        assert!(s.contains("\x1b["), "no VT sequences: {s:?}");
        // the process exited -> the waiter closed the console -> resize now refuses
        assert!(!pty.resize(80, 24));
    }
}
