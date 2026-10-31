//! Win32 helpers: dialog ownership/modality, native colour and font dialogs.
//! Non-Windows builds get inert stubs.

#[cfg(windows)]
mod imp {
    use windows_sys::core::BOOL;
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
    use windows_sys::Win32::Graphics::Gdi::{LOGFONTW, FW_BOLD};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Controls::Dialogs::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    struct Search {
        title: Option<String>,
        found: HWND,
    }

    unsafe extern "system" fn cb(h: HWND, lp: LPARAM) -> BOOL {
        let s = &mut *(lp as *mut Search);
        let mut buf = [0u16; 256];
        let n = GetWindowTextW(h, buf.as_mut_ptr(), 256);
        let t = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        let ok = match &s.title {
            Some(w) => &t == w,
            None => !t.is_empty() && IsWindowVisible(h) != 0,
        };
        if ok {
            s.found = h;
            return 0;
        }
        1
    }

    /// Find a top-level window of this thread by exact title (None = first visible titled window).
    pub fn find(title: Option<&str>) -> Option<isize> {
        let mut s = Search { title: title.map(|t| t.to_string()), found: std::ptr::null_mut() };
        unsafe { EnumThreadWindows(GetCurrentThreadId(), Some(cb), &mut s as *mut Search as LPARAM) };
        if s.found.is_null() { None } else { Some(s.found as isize) }
    }

    pub fn style_dialog(h: isize, owner: Option<isize>) {
        unsafe {
            let h = h as HWND;
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
            SetWindowLongPtrW(h, GWL_EXSTYLE, ex | WS_EX_DLGMODALFRAME as isize);
            // the original dialogs show no title-bar icon
            SendMessageW(h, WM_SETICON, ICON_SMALL as usize, 0);
            SendMessageW(h, WM_SETICON, ICON_BIG as usize, 0);
            if let Some(o) = owner {
                SetWindowLongPtrW(h, GWLP_HWNDPARENT, o);
            }
            SetWindowPos(h, std::ptr::null_mut(), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        }
    }

    pub fn enable(h: isize, on: bool) {
        unsafe {
            EnableWindow(h as HWND, on as BOOL);
        }
    }

    pub fn activate(h: isize) {
        unsafe {
            SetForegroundWindow(h as HWND);
        }
    }

    pub fn choose_color(owner: Option<isize>, init: u32) -> Option<u32> {
        unsafe {
            let mut custom = [0u32; 16];
            let mut cc: CHOOSECOLORW = std::mem::zeroed();
            cc.lStructSize = std::mem::size_of::<CHOOSECOLORW>() as u32;
            cc.hwndOwner = owner.unwrap_or(0) as HWND;
            cc.rgbResult = init;
            cc.lpCustColors = custom.as_mut_ptr();
            cc.Flags = CC_RGBINIT | CC_FULLOPEN;
            if ChooseColorW(&mut cc) != 0 { Some(cc.rgbResult) } else { None }
        }
    }

    /// (face name, size in points, style bits: 1 bold, 2 italic, 4 underline, 8 strikeout)
    pub fn choose_font(owner: Option<isize>, name: &str, size: i32, style: i32) -> Option<(String, i32, i32)> {
        unsafe {
            let mut lf: LOGFONTW = std::mem::zeroed();
            for (i, c) in name.encode_utf16().take(31).enumerate() {
                lf.lfFaceName[i] = c;
            }
            lf.lfHeight = -(size * 96 / 72);
            lf.lfCharSet = 128; // SHIFTJIS_CHARSET
            if style & 1 != 0 {
                lf.lfWeight = FW_BOLD as i32;
            }
            lf.lfItalic = (style & 2 != 0) as u8;
            lf.lfUnderline = (style & 4 != 0) as u8;
            lf.lfStrikeOut = (style & 8 != 0) as u8;
            let mut cf: CHOOSEFONTW = std::mem::zeroed();
            cf.lStructSize = std::mem::size_of::<CHOOSEFONTW>() as u32;
            cf.hwndOwner = owner.unwrap_or(0) as HWND;
            cf.lpLogFont = &mut lf;
            cf.Flags = CF_SCREENFONTS | CF_INITTOLOGFONTSTRUCT | CF_NOVERTFONTS;
            if ChooseFontW(&mut cf) == 0 {
                return None;
            }
            let n = lf.lfFaceName.iter().position(|c| *c == 0).unwrap_or(32);
            let face = String::from_utf16_lossy(&lf.lfFaceName[..n]);
            let pt = (cf.iPointSize / 10).max(1);
            let mut st = 0;
            if lf.lfWeight >= 600 {
                st |= 1;
            }
            if lf.lfItalic != 0 {
                st |= 2;
            }
            if lf.lfUnderline != 0 {
                st |= 4;
            }
            if lf.lfStrikeOut != 0 {
                st |= 8;
            }
            Some((face, pt, st))
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn find(_: Option<&str>) -> Option<isize> {
        None
    }
    pub fn style_dialog(_: isize, _: Option<isize>) {}
    pub fn enable(_: isize, _: bool) {}
    pub fn activate(_: isize) {}
    pub fn choose_color(_: Option<isize>, _: u32) -> Option<u32> {
        None
    }
    pub fn choose_font(_: Option<isize>, _: &str, _: i32, _: i32) -> Option<(String, i32, i32)> {
        None
    }
}

pub use imp::*;
