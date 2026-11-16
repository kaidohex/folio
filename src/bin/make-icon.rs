use image::{ImageBuffer, Rgba};

const NAVY: Rgba<u8> = Rgba([17, 24, 39, 255]);
const PAPER: Rgba<u8> = Rgba([250, 250, 248, 255]);
const CORAL: Rgba<u8> = Rgba([224, 90, 71, 255]);
const GRAY: Rgba<u8> = Rgba([200, 202, 208, 255]);
const CORAL_DIM: Rgba<u8> = Rgba([244, 200, 193, 255]);

fn main() {
    std::fs::create_dir_all("build").unwrap();
    for (size, path) in [
        (1024, "build/icon.png"),
        (256, "build/icon-256.png"),
        (128, "build/icon-128.png"),
    ] {
        draw(size).save(path).unwrap();
        println!("wrote {path}");
    }
    std::fs::write("build/icon.ico", ico_data()).unwrap();
    println!("wrote build/icon.ico");
}

fn rounded(x: u32, y: u32, s: u32, r: u32) -> bool {
    let in_corner = |dx: i64, dy: i64| dx * dx + dy * dy <= (r as i64) * (r as i64);
    if x < r && y < r {
        in_corner(x as i64 - r as i64, y as i64 - r as i64)
    } else if x >= s - r && y < r {
        in_corner((s - 1 - x) as i64 - r as i64, y as i64 - r as i64)
    } else if x < r && y >= s - r {
        in_corner(x as i64 - r as i64, (s - 1 - y) as i64 - r as i64)
    } else if x >= s - r && y >= s - r {
        in_corner((s - 1 - x) as i64 - r as i64, (s - 1 - y) as i64 - r as i64)
    } else {
        true
    }
}

fn draw(s: u32) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let mut img = ImageBuffer::new(s, s);
    let u = s as f32 / 128.0;
    let ux = |v: f32| (v * u).round() as u32;
    let r = ux(24.0);
    for y in 0..s {
        for x in 0..s {
            let mut px = Rgba([0, 0, 0, 0]);
            if rounded(x, y, s, r) {
                px = NAVY;
            }
            let (px0, py0, px1, py1) = (ux(28.0), ux(24.0), ux(100.0), ux(104.0));
            if x >= px0 && x < px1 && y >= py0 && y < py1 && px[3] > 0 {
                px = PAPER;
                let (bx0, bx1) = (ux(82.0), ux(94.0));
                let by1 = ux(52.0);
                if x >= bx0 && x < bx1 && y < by1 {
                    px = CORAL;
                    let notch_x = ux(88.0);
                    let depth = (x.max(notch_x) - notch_x).min(bx1 - notch_x);
                    if x >= notch_x && y > by1 - depth / 2 {
                        px = PAPER;
                    }
                }
                if y >= ux(34.0) && y < ux(38.0) && x >= ux(36.0) && x < ux(78.0) {
                    px = CORAL;
                }
                if y >= ux(40.0) && y < ux(44.0) && x >= ux(36.0) && x < ux(64.0) {
                    px = CORAL;
                }
                if x >= ux(36.0) && x < ux(60.0) && y >= ux(54.0) && y < ux(72.0) {
                    px = CORAL_DIM;
                }
                for (i, ly) in [76u32, 84, 92].iter().enumerate() {
                    let w = match i {
                        0 | 1 => 56.0,
                        _ => 40.0,
                    };
                    if y >= *ly && y < ly + ux(4.0) && x >= ux(36.0) && x < ux(w) {
                        px = GRAY;
                    }
                }
            }
            *img.get_pixel_mut(x, y) = px;
        }
    }
    img
}

fn ico_data() -> Vec<u8> {
    use std::io::Write;
    let sizes = [16u32, 32, 48, 256];
    let mut pngs = Vec::new();
    for sz in sizes {
        let img = draw(sz);
        let mut png = Vec::new();
        {
            let enc = image::codecs::png::PngEncoder::new(std::io::Cursor::new(&mut png));
            image::ImageEncoder::write_image(
                enc,
                img.as_raw(),
                sz,
                sz,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        }
        pngs.push(png);
    }
    let mut out = Vec::new();
    out.write_all(&[0, 0]).unwrap();
    out.write_all(&1u16.to_le_bytes()).unwrap();
    out.write_all(&(sizes.len() as u16).to_le_bytes()).unwrap();
    let mut offset: u32 = 6 + 16 * sizes.len() as u32;
    for (sz, png) in sizes.iter().zip(&pngs) {
        let b = if *sz >= 256 { 0u8 } else { *sz as u8 };
        out.write_all(&[b, b, 0, 0]).unwrap();
        out.write_all(&1u16.to_le_bytes()).unwrap();
        out.write_all(&32u16.to_le_bytes()).unwrap();
        out.write_all(&(png.len() as u32).to_le_bytes()).unwrap();
        out.write_all(&offset.to_le_bytes()).unwrap();
        offset += png.len() as u32;
    }
    for png in &pngs {
        out.extend_from_slice(png);
    }
    out
}
