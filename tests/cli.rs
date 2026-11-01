//! End-to-end checks of the command line path: the same render and export code the
//! dialogs use, without a window.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Temp(PathBuf);

impl Temp {
    fn new(tag: &str) -> Temp {
        let p = std::env::temp_dir().join(format!("avese-cli-{}-{}-{tag}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&p).unwrap();
        Temp(p)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn avese(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_avese")).args(args).output().expect("run avese")
}

/// A 21x21 grid with a bump in the middle.
fn write_grid(path: &Path) {
    let mut s = String::new();
    for j in 0..21 {
        for i in 0..21 {
            let (x, y) = (i as f64 * 0.5 - 5.0, j as f64 * 0.5 - 5.0);
            let z = (x * x + y * y).sqrt().sin() * 2.0 + (-(x * x + y * y) / 6.0).exp() * 3.0;
            s.push_str(&format!("{x:.3} {y:.3} {z:.3}\n"));
        }
    }
    std::fs::write(path, s).unwrap();
}

fn header(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap().into_iter().take(16).collect()
}

#[test]
fn render_writes_every_supported_image_format() {
    let t = Temp::new("render");
    let data = t.path("grid.txt");
    write_grid(&data);
    let data = data.to_str().unwrap();

    for (name, magic) in [("out.png", &b"\x89PNG"[..]), ("out.bmp", b"BM"), ("out.emf", b"\x01\x00\x00\x00"), ("out.wmf", b"\xd7\xcd\xc6\x9a")] {
        let out = t.path(name);
        let r = avese(&["--render", data, out.to_str().unwrap()]);
        assert!(r.status.success(), "{name}: {}", String::from_utf8_lossy(&r.stderr));
        let h = header(&out);
        assert_eq!(&h[..magic.len()], magic, "{name} header {h:02x?}");
        assert!(std::fs::metadata(&out).unwrap().len() > 1000, "{name} is empty");
    }

    // RLE8 keeps the same bitmap but a smaller file
    let (plain, rle) = (t.path("plain.bmp"), t.path("rle.bmp"));
    avese(&["--render", data, plain.to_str().unwrap()]);
    avese(&["--render", data, rle.to_str().unwrap(), "--rle"]);
    assert!(std::fs::metadata(&rle).unwrap().len() < std::fs::metadata(&plain).unwrap().len());
}

#[test]
fn settings_on_the_command_line_change_the_image() {
    let t = Temp::new("set");
    let data = t.path("grid.txt");
    write_grid(&data);
    let data = data.to_str().unwrap();
    let size = |args: &[&str]| {
        let out = t.path("s.png");
        let r = avese(&[&["--render", data, out.to_str().unwrap()], args].concat());
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        // IHDR: 8 signature bytes, 4 length, 4 type, then width and height
        let bytes = std::fs::read(&out).unwrap();
        let be = |o: usize| u32::from_be_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
        (be(16), be(20))
    };
    let base = size(&[]);
    assert_ne!(size(&["--set", "SEsize=200"]), base, "SEsize must resize the image");
    assert_ne!(size(&["--set", "EDv=2"]), base, "EDv must stretch the image");
    assert_ne!(size(&["--set", "EDh=0.5"]), base, "EDh must squeeze the image");
}

#[test]
fn avi_writes_uncompressed_frames_for_every_input() {
    let t = Temp::new("avi");
    let (a, b) = (t.path("a.txt"), t.path("b.txt"));
    write_grid(&a);
    write_grid(&b);
    let out = t.path("out.avi");
    let r = avese(&["--avi", out.to_str().unwrap(), "--fps", "10", a.to_str().unwrap(), b.to_str().unwrap()]);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"AVI ");
    // one frame in movi and one entry in the idx1 index per input file
    assert_eq!(bytes.windows(4).filter(|w| *w == b"00db").count(), 4, "two chunks per input file");
}

#[test]
fn a_missing_input_fails_without_writing_an_image() {
    let t = Temp::new("missing");
    let out = t.path("none.png");
    let r = avese(&["--render", t.path("absent.txt").to_str().unwrap(), out.to_str().unwrap()]);
    assert!(!r.status.success());
    assert!(!out.exists());
}
