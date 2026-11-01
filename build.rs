use image::{ColorType, ImageEncoder, codecs::png::PngEncoder};
use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=assets/generated/avese-logo.png");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let source = include_bytes!("assets/generated/avese-logo.png");
    let source = image::load_from_memory(source).expect("generated application logo is a valid PNG").to_rgba8();
    let sizes = [16u32, 32, 48, 64, 256];
    let mut images = Vec::<(u32, Vec<u8>)>::with_capacity(sizes.len());
    for size in sizes {
        let resized = image::imageops::resize(&source, size, size, image::imageops::FilterType::Lanczos3);
        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(resized.as_raw(), size, size, ColorType::Rgba8.into())
            .expect("encode PNG icon frame");
        images.push((size, png));
    }

    let icon = make_ico(&images);
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    let ico_path = out.join("avese.ico");
    fs::write(&ico_path, icon).expect("write Windows ICO resource");
    let rc_path = out.join("avese.rc");
    let ico_for_rc = ico_path.to_string_lossy().replace('\\', "/");
    fs::write(&rc_path, format!("1 ICON \"{ico_for_rc}\"\n")).expect("write Windows resource script");
    embed_resource::compile(&rc_path, embed_resource::NONE).manifest_optional().expect("embed Windows application icon");
}

fn make_ico(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let header_size = 6 + 16 * images.len();
    let payload_size: usize = images.iter().map(|(_, png)| png.len()).sum();
    let mut ico = Vec::with_capacity(header_size + payload_size);
    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = header_size as u32;
    for (size, png) in images {
        ico.push(if *size >= 256 { 0 } else { *size as u8 });
        ico.push(if *size >= 256 { 0 } else { *size as u8 });
        ico.push(0); // palette count: PNG truecolor
        ico.push(0); // reserved
        ico.extend_from_slice(&1u16.to_le_bytes()); // planes
        ico.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in images {
        ico.extend_from_slice(png);
    }
    ico
}
