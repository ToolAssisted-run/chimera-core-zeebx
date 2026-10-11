//! run-native <game> [--frames N] [--every N] [--press F:N:MASK]... [--pads N]
//!            [--rtc SECONDS] [--ppm FILE]
//!
//! Runs a game on the core built for the host, calling the same functions in
//! the same order as Chimera's engine does. Every so many frames it prints one
//! line: the game's clock, how many ARM instructions have run, and digests of
//! the picture and of all the sound so far. Two runs that print the same lines
//! were the same run.
//!
//!   --press F:N:MASK   hold the buttons in MASK (hexadecimal, one bit a
//!                      button in the order the package declares them) for N
//!                      frames starting at frame F
use zeebx_guest as core;

/// FNV-1a: small, and the same everywhere.
fn fnv(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
const FNV_START: u64 = 0xcbf29ce484222325;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: run-native <game> [--frames N] [--every N] [--press F:N:MASK]... [--pads N] [--rtc SECONDS] [--ppm FILE]");
        std::process::exit(2);
    }
    let mut frames = 600u64;
    let mut every = 60u64;
    let mut ppm: Option<String> = None;
    let mut presses: Vec<(u64, u64, u64)> = Vec::new();
    let mut settings = String::from("{");
    let mut i = 2;
    while i + 1 < args.len() {
        let value = &args[i + 1];
        match args[i].as_str() {
            "--frames" => frames = value.parse().expect("--frames N"),
            "--every" => every = value.parse().expect("--every N"),
            "--ppm" => ppm = Some(value.clone()),
            "--pads" => settings.push_str(&format!("\"pads\":\"{value}\",")),
            "--rtc" => settings.push_str(&format!("\"rtc_start\":\"{value}\",")),
            "--press" => {
                let parts: Vec<&str> = value.split(':').collect();
                assert!(parts.len() == 3, "--press F:N:MASK");
                presses.push((
                    parts[0].parse().expect("F"),
                    parts[1].parse().expect("N"),
                    u64::from_str_radix(parts[2], 16).expect("MASK in hexadecimal"),
                ));
            }
            other => {
                eprintln!("unknown option {other}");
                std::process::exit(2);
            }
        }
        i += 2;
    }
    if settings.ends_with(',') {
        settings.pop();
    }
    settings.push('}');
    let settings = core::Settings::parse(&settings).unwrap_or_else(|e| {
        eprintln!("run-native: {e}");
        std::process::exit(2);
    });
    if let Err(e) = core::start(&args[1], &settings) {
        eprintln!("run-native: {e}");
        std::process::exit(1);
    }
    let pixels = core::WIDTH * core::HEIGHT;
    let mut sound = FNV_START;
    let mut pairs = 0u64;
    let started = std::time::Instant::now();
    for frame in 1..=frames {
        let mut buttons = 0u64;
        for &(from, count, mask) in &presses {
            if frame >= from && frame < from + count {
                buttons |= mask;
            }
        }
        core::FrameAdvance(buttons);
        let count = core::GetAudioSampleCount() as usize;
        let audio = unsafe { std::slice::from_raw_parts(core::GetAudio() as *const u8, count * 4) };
        sound = fnv(sound, audio);
        pairs += count as u64;
        let running = core::IsRunning() != 0;
        if frame % every == 0 || frame == frames || !running {
            let video = unsafe { std::slice::from_raw_parts(core::GetVideoBgra() as *const u8, pixels * 4) };
            println!(
                "frame={frame} clockNs={} instructions={} audioPairs={pairs} audioHash={sound:016x} video={}x{} videoHash={:016x}",
                core::GetMachineTimeNs(),
                core::GetExecutedTicks(),
                core::WIDTH,
                core::HEIGHT,
                fnv(FNV_START, video)
            );
        }
        if !running {
            println!("stopped at frame {frame}");
            break;
        }
    }
    eprintln!("run-native: {:.1} s of host time", started.elapsed().as_secs_f64());
    if let Some(path) = ppm {
        let video = unsafe { std::slice::from_raw_parts(core::GetVideoBgra(), pixels) };
        let mut out = format!("P6\n{} {}\n255\n", core::WIDTH, core::HEIGHT).into_bytes();
        for p in video {
            out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
        }
        std::fs::write(&path, out).expect("the picture file");
    }
}
