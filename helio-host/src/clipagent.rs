//! Clipboard sharing on a Windows host. Helio Host runs as SYSTEM, and Windows
//! shows a SYSTEM process only part of the user's clipboard (text, but not
//! the files copied in Explorer); reading files as SYSTEM would also let a
//! user copy, and so send, what only SYSTEM may read. So the sharing runs in
//! a helper as the signed-in user -- `helio-host clipboard-agent`, started for the
//! session -- and Helio Host relays between it and the tunnel, over its standard
//! input and output:
//!
//! ```text
//! frame: kind u8 | length u32 LE | bytes
//! Helio Host -> agent  PACKET: a clipboard control body from the client
//!                      DIRECTIONS: which ways copies go from now on (one byte)
//! agent -> Helio Host  PACKET: a whole packet for the client;  LOG: a log line
//! ```
//!
//! The ways copies go at the start are on its command line.
//!
//! The helper runs as the user, so anything else running as them can write
//! what it says; Helio Host, as SYSTEM, is what puts it on the tunnel. So
//! the helper's word is checked here: only clipboard packets go to the
//! client, and copies only while the client may read them, whether or not
//! the helper has heard ([`for_client`]). A permission change goes to the
//! helper ahead of everything queued for it ([`write_frames`]).
//!
//! The agent ends when its input closes (the session ended).
//!
//! It is the engine in `C:\Program Files\Helio Host` (`hosting`), started as the
//! user from there only: a host running from anywhere else shares no
//! clipboard.

use std::io::{Read, Write};
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, SetHandleInformation, HANDLE, HANDLE_FLAGS, HANDLE_FLAG_INHERIT, WAIT_OBJECT_0,
};
use windows::Win32::Security::{
    DuplicateTokenEx, SecurityImpersonation, TokenPrimary, SECURITY_ATTRIBUTES, TOKEN_ALL_ACCESS,
};
use windows::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::RemoteDesktop::{WTSGetActiveConsoleSessionId, WTSQueryUserToken};
use windows::Win32::System::Threading::{
    CreateProcessAsUserW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
    TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject, CREATE_NO_WINDOW,
    CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, LPPROC_THREAD_ATTRIBUTE_LIST,
    PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, STARTF_USESTDHANDLES, STARTUPINFOEXW,
    STARTUPINFOW,
};

const PACKET: u8 = 0;
const LOG: u8 = 1;
const DIRECTIONS: u8 = 2;
/// A frame larger than this is not one of ours.
const MAX_FRAME: usize = 64 * 1024;

fn write_frame(w: &mut impl Write, kind: u8, bytes: &[u8]) -> std::io::Result<()> {
    let mut f = Vec::with_capacity(5 + bytes.len());
    f.push(kind);
    f.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    f.extend_from_slice(bytes);
    w.write_all(&f)?;
    w.flush()
}

fn read_frame(r: &mut impl Read) -> Option<(u8, Vec<u8>)> {
    let mut head = [0u8; 5];
    r.read_exact(&mut head).ok()?;
    let len = u32::from_le_bytes(head[1..5].try_into().ok()?) as usize;
    if len > MAX_FRAME {
        return None;
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).ok()?;
    Some((head[0], body))
}

/// What is queued for the helper.
enum ToHelper {
    /// A clipboard control body from the client.
    Packet(Vec<u8>),
    /// Nothing to write: wakes the writer to look at the directions.
    DirectionsChanged,
}

/// Pass what is queued to the helper, until the session or the helper ends.
/// The directions are read before each frame, so a change overtakes every
/// packet still queued and none of those lands against it. `told`: the ones
/// the helper already has.
fn write_frames(
    rx: Receiver<ToHelper>,
    directions: &AtomicU8,
    mut told: u8,
    to_child: &mut impl Write,
) {
    for next in rx {
        let now = directions.load(Ordering::Acquire);
        if now != told {
            if write_frame(to_child, DIRECTIONS, &[now]).is_err() {
                return;
            }
            told = now;
        }
        if let ToHelper::Packet(body) = next {
            if write_frame(to_child, PACKET, &body).is_err() {
                return;
            }
        }
    }
}

/// The ways copies go, from now on. Takes no time, so it holds up no session,
/// and cannot be lost: with the queue full the writer is not waiting for the
/// wake-up, and reads the directions before its next frame anyway.
fn tell_directions(tx: &Sender<ToHelper>, directions: &AtomicU8, bits: u8) {
    directions.store(bits, Ordering::Release);
    let _ = tx.try_send(ToHelper::DirectionsChanged);
}

/// Whether a packet from the helper goes on to the client.
fn for_client(packet: &[u8], directions: helio_clipboard::Directions) -> bool {
    match helio_proto::clip::from_packet(packet) {
        // A copy of the host's. Its acks and cancels carry none.
        Some(helio_proto::clip::Msg::Data { .. }) => directions.send,
        Some(_) => true,
        None => false,
    }
}

struct Process(HANDLE);

unsafe impl Send for Process {}

/// The session's clipboard helper, as Helio Host holds it.
pub struct ClipAgent {
    tx: Option<Sender<ToHelper>>,
    /// The ways copies go now, as `Directions::to_bits`.
    directions: Arc<AtomicU8>,
    process: Process,
    writer: Option<std::thread::JoinHandle<()>>,
    reader: Option<std::thread::JoinHandle<()>>,
}

impl ClipAgent {
    /// Start the helper as the user signed in at the console; `send` puts
    /// its packets on the tunnel.
    pub fn spawn(
        rate: u64,
        directions: helio_clipboard::Directions,
        send: impl Fn(&[u8]) + Send + 'static,
    ) -> Result<ClipAgent, String> {
        let exe = crate::hosting::trusted_engine()?;
        let cmdline = format!(
            "\"{}\" clipboard-agent {rate} {}",
            exe.display(),
            directions.to_bits()
        );
        let (process, to_child, from_child) = unsafe { spawn_as_user(&cmdline)? };
        let (tx, rx) = crossbeam_channel::bounded::<ToHelper>(8192);
        let told = directions.to_bits();
        let directions = Arc::new(AtomicU8::new(told));
        let now = directions.clone();
        let writer = std::thread::Builder::new()
            .name("clip-agent-in".into())
            .spawn(move || {
                let mut to_child = to_child;
                write_frames(rx, &now, told, &mut to_child);
            })
            .map_err(|e| e.to_string())?;
        let now = directions.clone();
        let reader = std::thread::Builder::new()
            .name("clip-agent-out".into())
            .spawn(move || {
                let mut from_child = std::io::BufReader::new(from_child);
                while let Some((kind, bytes)) = read_frame(&mut from_child) {
                    match kind {
                        PACKET => {
                            let d =
                                helio_clipboard::Directions::from_bits(now.load(Ordering::Acquire));
                            if for_client(&bytes, d) {
                                send(&bytes);
                            } else {
                                tracing::debug!("a packet from the clipboard agent held back");
                            }
                        }
                        LOG => tracing::info!(
                            "clipboard agent: {}",
                            String::from_utf8_lossy(&bytes).trim_end()
                        ),
                        _ => {}
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(ClipAgent {
            tx: Some(tx),
            directions,
            process,
            writer: Some(writer),
            reader: Some(reader),
        })
    }

    pub fn deliver(&self, body: &[u8]) {
        if let Some(tx) = &self.tx {
            let _ = tx.try_send(ToHelper::Packet(body.to_vec()));
        }
    }

    /// Which ways copies go, from now on.
    pub fn set_directions(&self, d: helio_clipboard::Directions) {
        if let Some(tx) = &self.tx {
            tell_directions(tx, &self.directions, d.to_bits());
        }
    }
}

impl Drop for ClipAgent {
    fn drop(&mut self) {
        // Its input closes: it stops. (The writer is joined after: one stuck
        // on a helper that stopped reading ends when the helper is ended.)
        drop(self.tx.take());
        unsafe {
            let started = Instant::now();
            if WaitForSingleObject(self.process.0, 3000) != WAIT_OBJECT_0 {
                tracing::warn!(
                    ms = started.elapsed().as_millis() as u64,
                    "the clipboard agent did not stop; ending it"
                );
                let _ = TerminateProcess(self.process.0, 1);
            }
            let _ = CloseHandle(self.process.0);
        }
        if let Some(w) = self.writer.take() {
            let _ = w.join();
        }
        if let Some(r) = self.reader.take() {
            let _ = r.join();
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// A pipe whose `child` end the child inherits; the other end is ours.
unsafe fn pipe(child_reads: bool) -> Result<(HANDLE, HANDLE), String> {
    let sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        bInheritHandle: true.into(),
        ..Default::default()
    };
    let (mut read, mut write) = (HANDLE::default(), HANDLE::default());
    CreatePipe(&mut read, &mut write, Some(&sa), 0).map_err(|e| format!("CreatePipe: {e}"))?;
    let (child, ours) = if child_reads {
        (read, write)
    } else {
        (write, read)
    };
    SetHandleInformation(ours, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0))
        .map_err(|e| format!("SetHandleInformation: {e}"))?;
    Ok((child, ours))
}

/// A process attribute list that names the only handles a child inherits.
struct InheritOnly {
    /// The list's memory, in `usize`s for its alignment.
    memory: Vec<usize>,
    /// The handles it names: read by CreateProcess, so kept, unmoved, for
    /// as long as the list.
    _handles: Box<[HANDLE; 2]>,
}

impl InheritOnly {
    /// # Safety
    /// `handles` are open and inheritable until the process is created.
    unsafe fn new(handles: [HANDLE; 2]) -> Result<InheritOnly, String> {
        let mut size = 0usize;
        // SAFETY: the first call only says how large the list is (and fails
        // for want of memory, as it must); the second initialises `memory`,
        // which is at least that large; the handles are boxed, so their
        // address holds until the list is deleted (`Drop`).
        unsafe {
            let _ = InitializeProcThreadAttributeList(None, 1, None, &mut size);
            let mut memory = vec![0usize; size.div_ceil(std::mem::size_of::<usize>())];
            let list = LPPROC_THREAD_ATTRIBUTE_LIST(memory.as_mut_ptr().cast());
            InitializeProcThreadAttributeList(Some(list), 1, None, &mut size)
                .map_err(|e| format!("InitializeProcThreadAttributeList: {e}"))?;
            let mut only = InheritOnly {
                memory,
                _handles: Box::new(handles),
            };
            UpdateProcThreadAttribute(
                only.list(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                Some(only._handles.as_ptr().cast()),
                std::mem::size_of::<[HANDLE; 2]>(),
                None,
                None,
            )
            .map_err(|e| format!("UpdateProcThreadAttribute: {e}"))?;
            Ok(only)
        }
    }

    fn list(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        LPPROC_THREAD_ATTRIBUTE_LIST(self.memory.as_mut_ptr().cast())
    }
}

impl Drop for InheritOnly {
    fn drop(&mut self) {
        // SAFETY: the list was initialised in `new` (an `InheritOnly`
        // exists only then) and is deleted once, here.
        unsafe { DeleteProcThreadAttributeList(self.list()) };
    }
}

/// Start `cmdline` as the console's user, its standard input and output
/// piped to us: (process, its input, its output).
unsafe fn spawn_as_user(cmdline: &str) -> Result<(Process, std::fs::File, std::fs::File), String> {
    let session = WTSGetActiveConsoleSessionId();
    let mut user = HANDLE::default();
    WTSQueryUserToken(session, &mut user).map_err(|e| format!("nobody is signed in ({e})"))?;
    let mut token = HANDLE::default();
    let dup = DuplicateTokenEx(
        user,
        TOKEN_ALL_ACCESS,
        None,
        SecurityImpersonation,
        TokenPrimary,
        &mut token,
    );
    let _ = CloseHandle(user);
    dup.map_err(|e| format!("DuplicateTokenEx: {e}"))?;

    let (child_in, our_in) = pipe(true)?;
    let (child_out, our_out) = pipe(false)?;
    // Only the two pipe ends cross to the user's process. Inheritance
    // alone would hand it every handle this SYSTEM process has open and
    // inheritable at that moment (a library's, the console's), for the
    // user to read or duplicate.
    let mut inherit = match InheritOnly::new([child_in, child_out]) {
        Ok(list) => list,
        Err(e) => {
            for h in [child_in, child_out, our_in, our_out, token] {
                let _ = CloseHandle(h);
            }
            return Err(e);
        }
    };
    let mut env: *mut std::ffi::c_void = std::ptr::null_mut();
    let have_env = CreateEnvironmentBlock(&mut env, Some(token), false).is_ok();
    let mut desktop = wide("winsta0\\default");
    let si = STARTUPINFOEXW {
        StartupInfo: STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOEXW>() as u32,
            lpDesktop: PWSTR(desktop.as_mut_ptr()),
            dwFlags: STARTF_USESTDHANDLES,
            hStdInput: child_in,
            hStdOutput: child_out,
            hStdError: HANDLE::default(),
            ..Default::default()
        },
        lpAttributeList: inherit.list(),
    };
    let mut cmd = wide(cmdline);
    let mut pi = PROCESS_INFORMATION::default();
    let result = CreateProcessAsUserW(
        Some(token),
        PCWSTR::null(),
        Some(PWSTR(cmd.as_mut_ptr())),
        None,
        None,
        true,
        CREATE_UNICODE_ENVIRONMENT | CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT,
        have_env.then_some(env as *const std::ffi::c_void),
        None,
        &si.StartupInfo,
        &mut pi,
    );
    drop(inherit);
    if have_env {
        let _ = DestroyEnvironmentBlock(env);
    }
    let _ = CloseHandle(token);
    // The child's ends are the child's now.
    let _ = CloseHandle(child_in);
    let _ = CloseHandle(child_out);
    if let Err(e) = result {
        let _ = CloseHandle(our_in);
        let _ = CloseHandle(our_out);
        return Err(format!("CreateProcessAsUserW: {e}"));
    }
    let _ = CloseHandle(pi.hThread);
    Ok((
        Process(pi.hProcess),
        std::fs::File::from_raw_handle(our_in.0 as RawHandle),
        std::fs::File::from_raw_handle(our_out.0 as RawHandle),
    ))
}

/// Log lines, sent to Helio Host as frames.
struct LogWriter {
    out: Arc<Mutex<std::io::Stdout>>,
    line: Vec<u8>,
}

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.line.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for LogWriter {
    fn drop(&mut self) {
        if !self.line.is_empty() {
            let _ = write_frame(&mut *self.out.lock(), LOG, &self.line);
        }
    }
}

/// `helio-host clipboard-agent RATE DIRECTIONS`: the helper itself, as the user.
pub fn run(args: &[String]) -> ExitCode {
    let rate = args.get(1).and_then(|r| r.parse().ok()).unwrap_or(4 << 20);
    // Nothing either way, unless Helio Host said.
    let directions = helio_clipboard::Directions::from_bits(
        args.get(2).and_then(|d| d.parse().ok()).unwrap_or(0),
    );
    let out = Arc::new(Mutex::new(std::io::stdout()));
    let log_out = out.clone();
    tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_target(false)
        .with_level(false)
        .with_writer(move || LogWriter {
            out: log_out.clone(),
            line: Vec::new(),
        })
        .init();
    let opts = helio_clipboard::Options {
        // The user's own temporary folder.
        files_dir: helio_clipboard::files_dir("Helio Host Clipboard"),
        rate,
        offer_current: false,
        peer: "the client".into(),
        directions,
    };
    let sync = helio_clipboard::ClipSync::start(opts, move |p| {
        let _ = write_frame(&mut *out.lock(), PACKET, p);
    });
    let mut input = std::io::BufReader::new(std::io::stdin().lock());
    while let Some((kind, body)) = read_frame(&mut input) {
        match kind {
            PACKET => sync.deliver(&body),
            DIRECTIONS => {
                if let Some(&bits) = body.first() {
                    sync.set_directions(helio_clipboard::Directions::from_bits(bits));
                }
            }
            _ => {}
        }
    }
    drop(sync);
    // Give the last log lines a moment to go out.
    std::thread::sleep(Duration::from_millis(50));
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use helio_clipboard::Directions;
    use helio_proto::clip::{cancel_packet, full_ack_packet, Outgoing};
    use helio_proto::control::{Control, EndReason, MAX_CONTROL_LEN};

    // To the writer they are only values to pass on.
    const BOTH: u8 = 3;
    const NOTHING: u8 = 0;

    /// The frames the writer gives the helper for what `tx` queued.
    fn written(tx: Sender<ToHelper>, rx: Receiver<ToHelper>, now: &AtomicU8) -> Vec<(u8, Vec<u8>)> {
        drop(tx);
        let mut out = Vec::new();
        write_frames(rx, now, BOTH, &mut out);
        let mut frames = Vec::new();
        let mut r = out.as_slice();
        while let Some(f) = read_frame(&mut r) {
            frames.push(f);
        }
        frames
    }

    #[test]
    fn packets_reach_the_helper_in_order_with_no_change_told() {
        let (tx, rx) = crossbeam_channel::bounded(8);
        let now = AtomicU8::new(BOTH);
        tx.send(ToHelper::Packet(vec![1])).unwrap();
        tx.send(ToHelper::Packet(vec![2])).unwrap();
        assert_eq!(
            written(tx, rx, &now),
            [(PACKET, vec![1]), (PACKET, vec![2])]
        );
    }

    #[test]
    fn a_permission_overtakes_the_packets_still_queued() {
        let (tx, rx) = crossbeam_channel::bounded(8);
        let now = AtomicU8::new(BOTH);
        tx.send(ToHelper::Packet(vec![1])).unwrap();
        tell_directions(&tx, &now, NOTHING);
        assert_eq!(
            written(tx, rx, &now),
            [(DIRECTIONS, vec![NOTHING]), (PACKET, vec![1])]
        );
    }

    #[test]
    fn a_permission_is_not_lost_behind_a_full_queue() {
        let (tx, rx) = crossbeam_channel::bounded(2);
        let now = AtomicU8::new(BOTH);
        tx.send(ToHelper::Packet(vec![1])).unwrap();
        tx.send(ToHelper::Packet(vec![2])).unwrap();
        tell_directions(&tx, &now, NOTHING);
        assert_eq!(
            written(tx, rx, &now),
            [
                (DIRECTIONS, vec![NOTHING]),
                (PACKET, vec![1]),
                (PACKET, vec![2])
            ]
        );
    }

    #[test]
    fn a_permission_reaches_a_helper_with_nothing_queued() {
        let (tx, rx) = crossbeam_channel::bounded(8);
        let now = AtomicU8::new(BOTH);
        tell_directions(&tx, &now, NOTHING);
        assert_eq!(written(tx, rx, &now), [(DIRECTIONS, vec![NOTHING])]);
    }

    #[test]
    fn a_change_and_back_tells_the_helper_nothing() {
        let (tx, rx) = crossbeam_channel::bounded(8);
        let now = AtomicU8::new(BOTH);
        tell_directions(&tx, &now, NOTHING);
        tell_directions(&tx, &now, BOTH);
        assert_eq!(written(tx, rx, &now), []);
    }

    fn data_packet() -> Vec<u8> {
        let now = Instant::now();
        Outgoing::new(7, vec![1, 2, 3], 50_000_000, now)
            .poll(now)
            .remove(0)
    }

    const NO_SEND: Directions = Directions {
        send: false,
        receive: true,
    };

    #[test]
    fn the_helpers_clipboard_packets_go_to_the_client() {
        assert!(for_client(&data_packet(), Directions::BOTH));
        assert!(for_client(&full_ack_packet(7, 3), Directions::BOTH));
        assert!(for_client(&cancel_packet(7), Directions::BOTH));
    }

    #[test]
    fn a_copy_stays_while_the_client_may_not_read_but_acks_and_cancels_go() {
        assert!(!for_client(&data_packet(), NO_SEND));
        assert!(for_client(&full_ack_packet(7, 3), NO_SEND));
        assert!(for_client(&cancel_packet(7), NO_SEND));
    }

    #[test]
    fn anything_else_from_the_helper_stays() {
        let mut out = [0u8; MAX_CONTROL_LEN];
        let n = Control::SessionEnd(EndReason::Quit).encode(0, &mut out);
        assert!(!for_client(&out[..n], Directions::BOTH));
        assert!(!for_client(&[], Directions::BOTH));
        assert!(!for_client(b"not a packet", Directions::BOTH));
    }
}
