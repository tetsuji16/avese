//! Rendering without a window:
//! `avese --render <data> <out> [--rle] [--ini <file>] [--set KEY=VALUE]...`
//! `avese --avi <out.avi> --fps <n> [--frame-size WxH] [--ini <file>] [--set KEY=VALUE]... <data>...`
use crate::engine::export::{self, ImageFormat};
use crate::engine::pipeline::{self, Rendered};
use crate::engine::settings::Settings;
use crate::ini;
use crate::state::Store;
use std::path::Path;

const USAGE: &str = "usage: avese --render <data> <out.png|bmp|emf|wmf> [--rle] [--ini <file>] [--set KEY=VALUE]...\n       avese --avi <out.avi> --fps <n> [--frame-size WxH] [--ini <file>] [--set KEY=VALUE]... <data>...";

#[derive(Default)]
struct Opts {
    pos: Vec<String>,
    ini_file: Option<String>,
    sets: Vec<(String, String)>,
    rle: bool,
    fps: Option<f64>,
    size: Option<(u32, u32)>,
    avi: Option<String>,
}

/// Returns the exit code when the arguments ask for a command line run, None to start the GUI.
pub fn run(args: Vec<String>) -> Option<i32> {
    let avi = match args.first().map(String::as_str) {
        Some("--render") => false,
        Some("--avi") => true,
        _ => return None,
    };
    #[cfg(all(windows, not(debug_assertions)))]
    unsafe {
        // the release build has no console of its own; use the parent's so that output is visible
        windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS);
    }
    let r = if avi { make_avi(&args) } else { render(&args[1..]) };
    Some(match r {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    })
}

fn parse(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--ini" => o.ini_file = Some(it.next().ok_or(USAGE)?.clone()),
            "--set" => {
                let kv = it.next().ok_or(USAGE)?;
                let (k, v) = kv.split_once('=').ok_or_else(|| format!("--set needs KEY=VALUE: {kv}"))?;
                o.sets.push((k.to_string(), v.to_string()));
            }
            "--rle" => o.rle = true,
            "--avi" => o.avi = Some(it.next().ok_or(USAGE)?.clone()),
            "--fps" => {
                let v = it.next().ok_or(USAGE)?;
                o.fps = Some(v.parse().map_err(|_| format!("--fps needs a number: {v}"))?);
            }
            "--frame-size" => {
                let v = it.next().ok_or(USAGE)?;
                let size = v.split_once(['x', 'X']).and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)));
                o.size = Some(size.filter(|s| s.0 > 0 && s.1 > 0).ok_or_else(|| format!("--frame-size needs WxH: {v}"))?);
            }
            _ => o.pos.push(a.clone()),
        }
    }
    Ok(o)
}

/// Settings from the default values, the --ini file and --set pairs.
fn base_store(o: &Opts) -> Result<Store, String> {
    let mut store = Settings::default_store();
    let mut layout = ini::Layout::default();
    if let Some(f) = &o.ini_file {
        ini::load_file(Path::new(f), &mut store, &mut layout).map_err(|e| format!("{f}: {e}"))?;
    }
    ini::apply_pairs(o.sets.iter().map(|(k, v)| (k.as_str(), v.as_str())), &mut store, &mut layout);
    Ok(store)
}

/// A fresh copy of the store per file, so settings embedded in one data file do not leak into the next.
fn render_file(data: &str, base: &Store) -> Result<Rendered, String> {
    let bytes = std::fs::read(data).map_err(|e| format!("{data}: {e}"))?;
    let mut store = base.clone();
    let dir = Path::new(data).parent().filter(|p| !p.as_os_str().is_empty());
    let loaded = pipeline::load(&bytes, &mut store, dir.or(Some(Path::new("."))), |_| {}).map_err(|e| format!("{data}: {e}"))?;
    for w in &loaded.warnings {
        eprintln!("warning: {w}");
    }
    pipeline::render(&loaded, &Settings::from_store(&store))
}

fn render(args: &[String]) -> Result<(), String> {
    let o = parse(args)?;
    let [data, out] = &o.pos[..] else { return Err(USAGE.into()) };
    let fmt = ImageFormat::from_extension(Path::new(out)).ok_or_else(|| format!("unknown output format (use .png, .bmp, .emf or .wmf): {out}"))?;
    let fmt = if fmt == ImageFormat::Bmp && o.rle { ImageFormat::RleBmp } else { fmt };
    let r = render_file(data, &base_store(&o)?)?;
    export::save_image(Path::new(out), &r, fmt)?;
    let [w, h] = r.size();
    println!("{w}x{h} ops={}", r.scene.ops.len());
    Ok(())
}

fn make_avi(args: &[String]) -> Result<(), String> {
    let o = parse(args)?;
    let (Some(out), Some(fps)) = (&o.avi, o.fps) else { return Err(USAGE.into()) };
    if o.pos.is_empty() {
        return Err(USAGE.into());
    }
    let base = base_store(&o)?;
    let (w, h) = match o.size {
        Some(s) => s,
        None => {
            let [w, h] = render_file(&o.pos[0], &base)?.size();
            (w as u32, h as u32)
        }
    };
    let n = o.pos.len();
    export::avi::write_avi(
        Path::new(out),
        n,
        w,
        h,
        fps,
        |i| {
            let r = render_file(&o.pos[i], &base)?;
            let [fw, fh] = r.size();
            Ok((r.rgba(), fw as u32, fh as u32))
        },
        |_| {},
    )?;
    println!("{n} frames {w}x{h}");
    Ok(())
}
