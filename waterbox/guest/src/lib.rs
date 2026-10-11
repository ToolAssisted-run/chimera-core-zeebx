//! The Zeebo core: the functions Chimera's engine calls, over the Zeebx
//! emulator (extern/zeebx) run a frame at a time.
//!
//! The same code is the native reference: waterbox/run-native links this crate
//! for the host and calls these functions in the order the engine does, so the
//! two builds differ only in where they run.

#![allow(non_snake_case)]
#![allow(clippy::missing_safety_doc)]

pub mod mathlib;

use serde_json::Value;
use std::ffi::{c_char, CStr, CString};
use std::path::Path;
use zeebx::audio::Mixer;
use zeebx::config::ZWheel;
use zeebx::input::bindings::Aparelho;
use zeebx::input::Pad;
use zeebx::session::{Session, Step};
use zeebx::storage::StoragePaths;

// The in-memory filesystem under the C library (waterbox/driver/memfs.cc):
// everything Zeebx unpacks and everything a game saves lives in it.
extern "C" {
    fn chimera_memfs_set_time(seconds: i64);
    fn chimera_memfs_mkdirs(path: *const c_char) -> i32;
    fn chimera_memfs_graft(path: *const c_char, host_name: *const c_char) -> i32;
    fn chimera_memfs_put(path: *const c_char, data: *const u8, size: usize) -> i32;
    fn chimera_memfs_list(dir: *const c_char) -> i32;
    fn chimera_memfs_list_name(index: i32) -> *const c_char;
    fn chimera_memfs_get(path: *const c_char) -> i64;
    fn chimera_memfs_get_bytes() -> *const u8;
}

/// Where the console keeps its files, in the in-memory tree.
const STORAGE_ROOT: &str = "/chimera/zeebx";
/// Where the game's package is seen from inside.
const GAME_DIR: &str = "/chimera/game";

// ---- The controllers, in waterbox.config's order -----------------------------

/// The console has two controller ports.
pub const PORTS: usize = 2;
/// A controller's buttons as the package declares them, each with Zeebx's own
/// name for it (`zeebx::input::BUTTON_NAMES`).
pub const BUTTONS: [(&str, &str); 14] = [
    ("Up", "up"),
    ("Down", "down"),
    ("Left", "left"),
    ("Right", "right"),
    ("1", "b1"),
    ("2", "b2"),
    ("3", "b3"),
    ("4", "b4"),
    ("ZL", "zl"),
    ("ZR", "zr"),
    ("Start", "start"),
    ("Back", "back"),
    ("Left Stick", "lthumb"),
    ("Right Stick", "rthumb"),
];
/// A controller's axes: the left stick's X and Y, then the right stick's.
/// -128..127 with 0 at rest; down and right are positive, as on the console.
pub const AXES_PER_PORT: usize = 4;

// ---- The picture and the sound ------------------------------------------------

pub const WIDTH: usize = 640;
pub const HEIGHT: usize = 480;
/// 60 frames a second, counted on the game's own clock.
pub const FRAMES_PER_SECOND: u64 = 60;
pub const SAMPLE_RATE: u32 = 44100;
/// 44.1 kHz stereo at 60 frames a second: the same number every frame.
pub const AUDIO_PAIRS_PER_FRAME: usize = (SAMPLE_RATE as u64 / FRAMES_PER_SECOND) as usize;

struct Core {
    session: Session,
    mixer: Mixer,
    frame: u64,
    running: bool,
    pads: usize,
    axes: [i32; PORTS * AXES_PER_PORT],
    /// Zeebx's button index for each declared button.
    button_index: [usize; BUTTONS.len()],
    video: Vec<u32>,
    audio: Vec<i16>,
    /// The save data listed by the last GetSaveDataFileCount, names
    /// NUL-terminated.
    saves: Vec<(Vec<u8>, Vec<u8>)>,
}

static mut CORE: Option<Core> = None;
static mut LOAD_ERROR: Vec<u8> = Vec::new();
static mut BLANK: Vec<u32> = Vec::new();

#[allow(static_mut_refs)]
fn core() -> Option<&'static mut Core> {
    unsafe { CORE.as_mut() }
}

#[allow(static_mut_refs)]
fn fail(why: String) -> i32 {
    eprintln!("zeebx core: {why}");
    unsafe {
        LOAD_ERROR = CString::new(why.replace('\0', " ")).unwrap().into_bytes_with_nul();
    }
    0
}

// ---- Settings -------------------------------------------------------------------

/// The settings the engine mounts ("settings", a JSON object with every
/// declared setting in it).
pub struct Settings {
    /// The date and time the console is switched on, in seconds since 1970.
    pub rtc_start: u64,
    /// How many controllers are plugged in: 1 or 2.
    pub pads: usize,
}

fn as_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

impl Settings {
    pub fn parse(json: &str) -> Result<Settings, String> {
        let mut s = Settings {
            rtc_start: 1_262_304_000, // 2010-01-01 00:00:00 UTC
            pads: 1,
        };
        let root: Value = if json.trim().is_empty() {
            Value::Object(Default::default())
        } else {
            serde_json::from_str(json).map_err(|e| format!("the settings are not JSON: {e}"))?
        };
        let Value::Object(map) = root else {
            return Err("the settings are not a JSON object".to_string());
        };
        for (key, value) in &map {
            let text = as_text(value);
            match key.as_str() {
                "rtc_start" => {
                    // not before 1980-01-06: the console's own calendar starts there
                    s.rtc_start = text
                        .parse::<u64>()
                        .ok()
                        .filter(|&t| t >= 315_964_800)
                        .ok_or_else(|| format!("rtc_start: {text:?} is not a time from 1980-01-06 on"))?
                }
                "pads" => {
                    s.pads = text
                        .parse::<usize>()
                        .ok()
                        .filter(|p| (1..=PORTS).contains(p))
                        .ok_or_else(|| format!("pads: {text:?} is not 1 or 2"))?
                }
                _ => {} // a setting this build does not know is not an error
            }
        }
        Ok(s)
    }
}

// ---- Save data in ----------------------------------------------------------------

/// The files of a save-data zip (what Chimera's Export Save Data writes):
/// paths under the console's saves folder, as GetSaveDataFileName names them.
pub fn read_save_zip(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("not a zip: {e}"))?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        if f.is_dir() {
            continue;
        }
        let name = f.name().replace('\\', "/");
        // a name that would leave the saves folder is not a save
        if name.starts_with('/') || name.split('/').any(|part| part == ".." || part.is_empty()) {
            return Err(format!("{name}: not a path inside the saves folder"));
        }
        let mut data = Vec::new();
        f.read_to_end(&mut data).map_err(|e| format!("{name}: {e}"))?;
        out.push((name, data));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

// ---- Starting --------------------------------------------------------------------

fn c(text: &str) -> CString {
    CString::new(text.replace('\0', " ")).unwrap()
}

/// Make the console with the game in it. `host_name` is where the game's
/// package is read from: a path natively, a mounted file's name in the
/// sandbox. Nothing of the game runs until the first frame.
#[allow(static_mut_refs)]
pub fn start(host_name: &str, settings: &Settings, save_data: Vec<(String, Vec<u8>)>) -> Result<(), String> {
    let base = host_name.rsplit(['/', '\\']).next().unwrap_or(host_name);
    start_named(host_name, base, settings, save_data)
}

/// As [`start`], when the package is read under one name and is called
/// another: `base` is the game's own file name, which says what kind of
/// package it is.
#[allow(static_mut_refs)]
pub fn start_named(
    host_name: &str,
    base: &str,
    settings: &Settings,
    save_data: Vec<(String, Vec<u8>)>,
) -> Result<(), String> {
    let lower = base.to_ascii_lowercase();
    if !(lower.ends_with(".7z") || lower.ends_with(".zip") || lower.ends_with(".mod")) {
        return Err(format!("{base} is not a .7z, .zip or .mod file: this core runs Zeebo games packaged that way"));
    }
    let inside = format!("{GAME_DIR}/{base}");
    unsafe {
        // every file the console makes is dated the moment it was switched on
        chimera_memfs_set_time(settings.rtc_start as i64);
        if chimera_memfs_mkdirs(c(STORAGE_ROOT).as_ptr()) == 0 || chimera_memfs_mkdirs(c(GAME_DIR).as_ptr()) == 0 {
            return Err("the console's storage could not be made".to_string());
        }
        if chimera_memfs_graft(c(&inside).as_ptr(), c(host_name).as_ptr()) == 0 {
            return Err(format!("{host_name} could not be opened"));
        }
    }
    // what was saved before goes back where the console keeps it, before
    // the game starts
    for (name, bytes) in &save_data {
        let path = format!("{STORAGE_ROOT}/saves/{name}");
        if unsafe { chimera_memfs_put(c(&path).as_ptr(), bytes.as_ptr(), bytes.len()) } == 0 {
            return Err(format!("the save data: {name} could not be put back"));
        }
    }
    zeebx::machine::FIXED_START_UNIX_SECONDS.store(settings.rtc_start, std::sync::atomic::Ordering::Relaxed);

    let storage = StoragePaths::from_root(STORAGE_ROOT);
    storage.create_dirs().map_err(|e| format!("the console's storage: {e}"))?;
    let mut ports = [None; zeebx::input::PORTAS];
    for port in ports.iter_mut().take(settings.pads) {
        *port = Some(Aparelho::Controle);
    }
    let mut session = Session::start_software_with_storage(Path::new(&inside), ports, ZWheel::default(), &storage)
        .map_err(|e| format!("the game did not start: {e}"))?;
    let mixer = session.grava_audio(SAMPLE_RATE);

    let mut button_index = [0usize; BUTTONS.len()];
    for (i, (_, name)) in BUTTONS.iter().enumerate() {
        button_index[i] = Pad::button_by_name(name).ok_or_else(|| format!("Zeebx has no button named {name}"))?;
    }
    unsafe {
        CORE = Some(Core {
            session,
            mixer,
            frame: 0,
            running: true,
            pads: settings.pads,
            axes: [0; PORTS * AXES_PER_PORT],
            button_index,
            video: vec![0xff00_0000; WIDTH * HEIGHT],
            audio: vec![0; AUDIO_PAIRS_PER_FRAME * 2],
            saves: Vec::new(),
        });
    }
    Ok(())
}

fn read_mount(name: &str) -> Result<Vec<u8>, String> {
    std::fs::read(name).map_err(|e| format!("{name}: {e}"))
}

/// The first file of slot `id` in the project's slot map ("slots": a JSON
/// object of slot id to file names, each file mounted under its own name).
fn slot_first(id: &str) -> Option<String> {
    let text = std::fs::read("slots").ok()?;
    let map: Value = serde_json::from_slice(&text).ok()?;
    map.get(id)?.get(0)?.as_str().map(String::from)
}

#[no_mangle]
pub extern "C" fn Init() -> i32 {
    let settings = match read_mount("settings")
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .and_then(|s| Settings::parse(&s))
    {
        Ok(s) => s,
        Err(e) => return fail(e),
    };
    // The game: a project names it in its "game" slot; a file opened on its
    // own arrives with its name in rom.name (and mounted under it too).
    let name = match slot_first("game") {
        Some(n) => n,
        None => match read_mount("rom.name") {
            Ok(n) => String::from_utf8_lossy(&n).trim().to_string(),
            Err(e) => return fail(format!("no game: {e}")),
        },
    };
    // The engine mounts the file under its own name, with or without the
    // folder it came from, and under "rom" as well: the first that is there.
    let base = name.rsplit(['/', '\\']).next().unwrap_or(&name).to_string();
    let mounted = [base.as_str(), name.as_str(), "rom"]
        .into_iter()
        .find(|candidate| Path::new(candidate).is_file())
        .unwrap_or(base.as_str())
        .to_string();
    // What the console kept before: the .zip Export Save Data writes.
    let save_data = match slot_first("savedata") {
        None => Vec::new(),
        Some(zip_name) => {
            let zip_base = zip_name.rsplit(['/', '\\']).next().unwrap_or(&zip_name).to_string();
            match read_mount(&zip_base).and_then(|b| read_save_zip(&b)) {
                Ok(files) => files,
                Err(e) => return fail(format!("the save data {zip_base}: {e}")),
            }
        }
    };
    match start_named(&mounted, &base, &settings, save_data) {
        Ok(()) => 1,
        Err(e) => fail(e),
    }
}

#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn GetLoadError() -> *const u8 {
    unsafe {
        if LOAD_ERROR.is_empty() {
            b"\0".as_ptr()
        } else {
            LOAD_ERROR.as_ptr()
        }
    }
}

// ---- A frame -----------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn SetAxis(index: i32, value: i32) {
    if let Some(core) = core() {
        if let Some(axis) = core.axes.get_mut(index as usize) {
            *axis = value;
        }
    }
}

/// One frame: the buttons are one bit each, the first port's 14 lowest and
/// the second port's after them, in the order the package declares them.
#[no_mangle]
pub extern "C" fn FrameAdvance(buttons: u64) {
    let Some(core) = core() else { return };
    if !core.running {
        return;
    }
    for port in 0..core.pads {
        let mut pad = Pad::default();
        for (i, &index) in core.button_index.iter().enumerate() {
            if buttons >> (port * BUTTONS.len() + i) & 1 != 0 {
                pad.press(index, true);
            }
        }
        for axis in 0..AXES_PER_PORT {
            pad.set_axis(axis, core.axes[port * AXES_PER_PORT + axis]);
        }
        core.session.set_port_pad(port, pad);
    }
    // Frame n ends when the game's clock reaches n/60 of a second.
    core.frame += 1;
    let target_ms = core.frame * 1000 / FRAMES_PER_SECOND;
    if let Step::Stopped = core.session.run_until_ms(target_ms) {
        core.running = false;
        eprintln!(
            "zeebx core: the game stopped at frame {}: {}",
            core.frame,
            core.session.stopped_reason().unwrap_or_default()
        );
    }
    // the picture: 16 bits a pixel on the console, 32 here with blue lowest
    let screen = core.session.screen();
    if screen.width() as usize == WIDTH && screen.height() as usize == HEIGHT {
        for (out, &p) in core.video.iter_mut().zip(screen.pixels()) {
            let (r, g, b) = (u32::from(p >> 11) & 31, u32::from(p >> 5) & 63, u32::from(p) & 31);
            *out = 0xff00_0000 | (r << 3 | r >> 2) << 16 | (g << 2 | g >> 4) << 8 | (b << 3 | b >> 2);
        }
    }
    // the sound of this frame
    let mixed = core.mixer.render(AUDIO_PAIRS_PER_FRAME);
    for (out, &s) in core.audio.iter_mut().zip(mixed.iter()) {
        *out = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
    }
}

#[no_mangle]
pub extern "C" fn IsRunning() -> i32 {
    core().map(|c| c.running as i32).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn GetFrameCount() -> u64 {
    core().map(|c| c.frame).unwrap_or(0)
}

/// The game's clock, in nanoseconds since it started.
#[no_mangle]
pub extern "C" fn GetMachineTimeNs() -> u64 {
    core().map(|c| u64::from(c.session.clock_ms()) * 1_000_000).unwrap_or(0)
}

/// How many ARM instructions the game has run.
#[no_mangle]
pub extern "C" fn GetExecutedTicks() -> u64 {
    core().map(|c| c.session.instrucoes()).unwrap_or(0)
}

#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn GetVideoBgra() -> *const u32 {
    match core() {
        Some(c) => c.video.as_ptr(),
        None => unsafe {
            if BLANK.is_empty() {
                BLANK = vec![0xff00_0000; WIDTH * HEIGHT];
            }
            BLANK.as_ptr()
        },
    }
}
#[no_mangle]
pub extern "C" fn GetVideoWidth() -> i32 {
    WIDTH as i32
}
#[no_mangle]
pub extern "C" fn GetVideoHeight() -> i32 {
    HEIGHT as i32
}
#[no_mangle]
pub extern "C" fn GetVsyncNumerator() -> i32 {
    FRAMES_PER_SECOND as i32
}
#[no_mangle]
pub extern "C" fn GetVsyncDenominator() -> i32 {
    1
}

#[no_mangle]
pub extern "C" fn GetAudio() -> *const i16 {
    core().map(|c| c.audio.as_ptr()).unwrap_or(std::ptr::null())
}
#[no_mangle]
pub extern "C" fn GetAudioSampleCount() -> i32 {
    core().map(|_| AUDIO_PAIRS_PER_FRAME as i32).unwrap_or(0)
}

/// Which of the declared controls this console has: the controllers that are
/// plugged in. One that is not gets no column in the input log.
#[no_mangle]
pub extern "C" fn IsButtonActive(index: i32) -> i32 {
    core().map(|c| (index >= 0 && (index as usize) / BUTTONS.len() < c.pads) as i32).unwrap_or(0)
}
#[no_mangle]
pub extern "C" fn IsAxisActive(index: i32) -> i32 {
    core().map(|c| (index >= 0 && (index as usize) / AXES_PER_PORT < c.pads) as i32).unwrap_or(0)
}

// ---- Save data out ---------------------------------------------------------------

/// What the console keeps: every file a game wrote, as it stands now. The
/// count; then each file by index until the next call.
#[no_mangle]
pub extern "C" fn GetSaveDataFileCount() -> i32 {
    let Some(core) = core() else { return 0 };
    core.saves.clear();
    let root = format!("{STORAGE_ROOT}/saves");
    unsafe {
        let count = chimera_memfs_list(c(&root).as_ptr());
        let names: Vec<String> =
            (0..count).map(|i| CStr::from_ptr(chimera_memfs_list_name(i)).to_string_lossy().into_owned()).collect();
        for name in names {
            let size = chimera_memfs_get(c(&format!("{root}/{name}")).as_ptr());
            if size < 0 {
                continue;
            }
            let bytes = std::slice::from_raw_parts(chimera_memfs_get_bytes(), size as usize).to_vec();
            core.saves.push((c(&name).into_bytes_with_nul(), bytes));
        }
    }
    core.saves.len() as i32
}
#[no_mangle]
pub extern "C" fn GetSaveDataFileName(index: i32) -> *const u8 {
    core().and_then(|c| c.saves.get(index as usize)).map(|s| s.0.as_ptr()).unwrap_or(b"\0".as_ptr())
}
#[no_mangle]
pub extern "C" fn GetSaveDataFileSize(index: i32) -> u64 {
    core().and_then(|c| c.saves.get(index as usize)).map(|s| s.1.len() as u64).unwrap_or(0)
}
#[no_mangle]
pub extern "C" fn GetSaveDataFileBuffer(index: i32) -> *const u8 {
    core().and_then(|c| c.saves.get(index as usize)).map(|s| s.1.as_ptr()).unwrap_or(std::ptr::null())
}

// ---- Memory ------------------------------------------------------------------------

// No memory domains: the game's memory is offered as a bus, below.
#[no_mangle]
pub extern "C" fn GetMemoryDomainCount() -> i32 {
    0
}
#[no_mangle]
pub extern "C" fn GetMemoryDomainName(_index: i32) -> *const u8 {
    b"\0".as_ptr()
}
#[no_mangle]
pub extern "C" fn GetMemoryDomainPtr(_index: i32) -> *mut u8 {
    std::ptr::null_mut()
}
#[no_mangle]
pub extern "C" fn GetMemoryDomainSize(_index: i32) -> u64 {
    0
}
#[no_mangle]
pub extern "C" fn GetMemoryDomainWritable(_index: i32) -> i32 {
    0
}

// The game's 32-bit address space, as the game sees it. Most of it is empty:
// GetBusRanges says where the game's writable memory is (its data, its heap,
// its stack and the console's objects), so that a search looks only there.

static mut BUS_RUN: [u8; 65536] = [0; 65536];
static mut BUS_RANGES: Vec<i64> = Vec::new();

#[no_mangle]
pub extern "C" fn GetBusCount() -> i32 {
    core().map(|_| 1).unwrap_or(0)
}
#[no_mangle]
pub extern "C" fn GetBusName(_index: i32) -> *const u8 {
    b"Memory\0".as_ptr()
}
#[no_mangle]
pub extern "C" fn GetBusSize(_index: i32) -> i64 {
    1i64 << 32
}
#[no_mangle]
pub extern "C" fn GetBusWritable(_index: i32) -> i32 {
    1
}
#[no_mangle]
pub extern "C" fn PeekBus(bus: i32, addr: i32) -> i32 {
    let mut byte = [0u8; 1];
    if let (0, Some(core)) = (bus, core()) {
        core.session.chimera_read(addr as u32, &mut byte);
    }
    i32::from(byte[0])
}
#[no_mangle]
pub extern "C" fn PokeBus(bus: i32, addr: i32, value: i32) {
    if let (0, Some(core)) = (bus, core()) {
        core.session.chimera_write(addr as u32, &[value as u8]);
    }
}
/// `len` bytes from `addr` (at most 65536), until the next call. Where there
/// is no memory the bytes are zero.
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn ReadBus(bus: i32, addr: i64, len: i32) -> *const u8 {
    let run = unsafe { &mut BUS_RUN };
    let len = (len.max(0) as usize).min(run.len());
    run[..len].fill(0);
    if let (0, Some(core)) = (bus, core()) {
        if addr >= 0 && addr + len as i64 <= 1i64 << 32 {
            // in one piece when the run lies in one range of memory, a page
            // at a time when it does not
            if !core.session.chimera_read(addr as u32, &mut run[..len]) {
                run[..len].fill(0);
                let mut at = 0usize;
                while at < len {
                    let page_end = (((addr as usize + at) | 0xfff) + 1 - addr as usize).min(len);
                    if !core.session.chimera_read((addr as usize + at) as u32, &mut run[at..page_end]) {
                        run[at..page_end].fill(0);
                    }
                    at = page_end;
                }
            }
        }
    }
    run.as_ptr()
}
/// Where the bus has memory a game can write: (address, length) pairs, ended
/// by a pair of no length.
#[no_mangle]
#[allow(static_mut_refs)]
pub extern "C" fn GetBusRanges(bus: i32) -> *const i64 {
    let ranges = unsafe { &mut BUS_RANGES };
    ranges.clear();
    if let (0, Some(core)) = (bus, core()) {
        for (base, length) in core.session.chimera_ranges() {
            ranges.push(i64::from(base));
            ranges.push(i64::from(length));
        }
    }
    ranges.push(0);
    ranges.push(0);
    ranges.as_ptr()
}
