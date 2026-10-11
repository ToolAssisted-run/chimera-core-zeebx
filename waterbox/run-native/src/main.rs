//! run-native <game> <new work folder> [--frames N] [--every N] [--ppm FILE]
//!
//! Starts a game on the emulator's software renderer and runs it a frame at a
//! time. Every so many frames it prints the emulator's clock, how many ARM
//! instructions have run, and a digest of the picture and of the sound.
use std::hash::Hasher;
use std::path::Path;
use zeebx::config::ZWheel;
use zeebx::input::bindings::Aparelho;
use zeebx::session::{Session, Step};
use zeebx::storage::StoragePaths;

fn digest(bytes: impl Iterator<Item = u8>) -> u64 {
    // FNV-1a: small, and the same everywhere
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: run-native <game> <new work folder> [--frames N] [--every N] [--ppm FILE]");
        std::process::exit(2);
    }
    let mut frames = 600u64;
    let mut every = 60u64;
    let mut ppm: Option<String> = None;
    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "--frames" => { frames = args[i + 1].parse().unwrap(); i += 2; }
            "--every" => { every = args[i + 1].parse().unwrap(); i += 2; }
            "--ppm" => { ppm = Some(args[i + 1].clone()); i += 2; }
            other => { eprintln!("unknown option {other}"); std::process::exit(2); }
        }
    }
    let storage = StoragePaths::from_root(&args[2]);
    storage.create_dirs().expect("the work folder");
    let ports = [Some(Aparelho::Controle), None];
    let mut session = match Session::start_software_with_storage(Path::new(&args[1]), ports, ZWheel::default(), &storage) {
        Ok(s) => s,
        Err(e) => { eprintln!("run-native: the game did not start: {e}"); std::process::exit(1); }
    };
    let mixer = session.grava_audio(44100);
    let mut sound = std::collections::hash_map::DefaultHasher::new();
    let mut pairs = 0u64;
    let started = std::time::Instant::now();
    let mut clock_before = session.clock_ms();
    for frame in 1..=frames {
        let step = if session.mostra_quadro_intermediario() { Step::Presented } else { session.run_frame(false) };
        let clock = session.clock_ms();
        let due = (u64::from(clock.wrapping_sub(clock_before)) * 44100 / 1000) as usize;
        clock_before = clock;
        for s in mixer.render(due) {
            sound.write_u32(s.to_bits());
        }
        pairs += due as u64;
        if frame % every == 0 || frame == frames || matches!(step, Step::Stopped) {
            let screen = session.screen();
            let picture = digest(screen.pixels().iter().flat_map(|p| p.to_le_bytes()));
            println!(
                "frame={frame} clockMs={clock} instructions={} presented={} audioPairs={pairs} audioHash={:016x} video={}x{} videoHash={picture:016x}",
                session.instrucoes(), session.quadros_apresentados(), sound.finish(), screen.width(), screen.height()
            );
        }
        if matches!(step, Step::Stopped) {
            println!("stopped: {}", session.stopped_reason().unwrap_or_default());
            break;
        }
    }
    eprintln!("run-native: {:.1} s of host time", started.elapsed().as_secs_f64());
    if let Some(path) = ppm {
        let screen = session.screen();
        let mut out = format!("P6\n{} {}\n255\n", screen.width(), screen.height()).into_bytes();
        for p in screen.pixels() {
            let (r, g, b) = ((p >> 11) & 31, (p >> 5) & 63, p & 31);
            out.extend_from_slice(&[(r << 3 | r >> 2) as u8, (g << 2 | g >> 4) as u8, (b << 3 | b >> 2) as u8]);
        }
        std::fs::write(&path, out).expect("the picture file");
    }
}
