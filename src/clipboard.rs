//! Copies the rendering to the Windows clipboard as CF_DIB or CF_ENHMETAFILE.
use crate::engine::export::bmp::{Indexed, dib_bytes};

#[allow(dead_code)]
pub fn copy_bitmap(img: &Indexed) -> Result<(), String> {
    imp::copy_dib(&dib_bytes(img))
}

#[allow(dead_code)]
pub fn copy_emf(emf: &[u8]) -> Result<(), String> {
    imp::copy_emf(emf)
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use windows_sys::Win32::Graphics::Gdi::{DeleteEnhMetaFile, SetEnhMetaFileBits};
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};

    const CF_DIB: u32 = 8;
    const CF_ENHMETAFILE: u32 = 14;

    /// Another process may hold the clipboard for a moment.
    struct Open;

    impl Open {
        fn new() -> Result<Open, String> {
            for i in 0..10 {
                if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
                    return Ok(Open);
                }
                if i < 9 {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
            Err("クリップボードを開けません（他のアプリが使用中です）".into())
        }
    }

    impl Drop for Open {
        fn drop(&mut self) {
            unsafe { CloseClipboard() };
        }
    }

    pub fn copy_dib(dib: &[u8]) -> Result<(), String> {
        let _open = Open::new()?;
        unsafe {
            let mem = GlobalAlloc(GMEM_MOVEABLE, dib.len());
            if mem.is_null() {
                return Err("クリップボード用のメモリを確保できません".into());
            }
            let p = GlobalLock(mem) as *mut u8;
            if p.is_null() {
                GlobalFree(mem);
                return Err("クリップボード用のメモリをロックできません".into());
            }
            std::ptr::copy_nonoverlapping(dib.as_ptr(), p, dib.len());
            GlobalUnlock(mem);
            if EmptyClipboard() == 0 {
                GlobalFree(mem);
                return Err("クリップボードを空にできません".into());
            }
            if SetClipboardData(CF_DIB, mem as *mut c_void).is_null() {
                GlobalFree(mem);
                return Err("クリップボードにコピーできません".into());
            }
        }
        Ok(())
    }

    pub fn copy_emf(emf: &[u8]) -> Result<(), String> {
        let _open = Open::new()?;
        unsafe {
            let h = SetEnhMetaFileBits(emf.len() as u32, emf.as_ptr());
            if h.is_null() {
                return Err("メタファイルを作成できません".into());
            }
            if EmptyClipboard() == 0 {
                DeleteEnhMetaFile(h);
                return Err("クリップボードを空にできません".into());
            }
            if SetClipboardData(CF_ENHMETAFILE, h as *mut c_void).is_null() {
                DeleteEnhMetaFile(h);
                return Err("クリップボードにコピーできません".into());
            }
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn copy_dib(_: &[u8]) -> Result<(), String> {
        Err("このシステムでは未対応です".into())
    }

    pub fn copy_emf(_: &[u8]) -> Result<(), String> {
        Err("このシステムでは未対応です".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dib_has_no_file_header() {
        let img = Indexed { w: 2, h: 1, palette: vec![[1, 2, 3]], pixels: vec![0, 0] };
        let d = dib_bytes(&img);
        assert_eq!(&d[0..4], &40u32.to_le_bytes());
    }

    /// Alters the real clipboard; run by hand with `cargo test -- --ignored clipboard_`.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn clipboard_dib() {
        let img = Indexed { w: 4, h: 2, palette: vec![[255, 0, 0], [0, 0, 255]], pixels: vec![0, 0, 1, 1, 1, 1, 0, 0] };
        copy_bitmap(&img).unwrap();
    }

    /// Alters the real clipboard; see `clipboard_dib`.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn clipboard_emf() {
        let s = crate::engine::scene::Scene { width: 40, height: 30, bg: crate::engine::scene::Rgb(0, 128, 0), ops: vec![] };
        copy_emf(&crate::engine::export::emf::encode_emf(&s)).unwrap();
    }
}
