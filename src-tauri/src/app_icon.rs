use tauri::{image::Image, AppHandle, Manager};

#[derive(Debug, Clone, Copy)]
struct IconPalette {
    top: [f32; 3],
    bottom: [f32; 3],
}

const ICON_SIZE: u32 = 128;

pub fn apply_app_icon(app: &AppHandle, style: &str) -> Result<(), String> {
    let image = render_app_icon(style, ICON_SIZE);
    for window in app.webview_windows().into_values() {
        let _ = window.set_icon(image.clone());
    }
    Ok(())
}

fn render_app_icon(style: &str, size: u32) -> Image<'static> {
    let palette = icon_palette(style);
    let mut pixels = vec![0_u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let color = icon_pixel(x as f32 + 0.5, y as f32 + 0.5, size as f32, palette);
            let offset = ((y * size + x) * 4) as usize;
            pixels[offset] = to_u8(color[0]);
            pixels[offset + 1] = to_u8(color[1]);
            pixels[offset + 2] = to_u8(color[2]);
            pixels[offset + 3] = to_u8(color[3]);
        }
    }
    Image::new_owned(pixels, size, size)
}

fn icon_pixel(x: f32, y: f32, size: f32, palette: IconPalette) -> [f32; 4] {
    let inset = size * 0.04;
    let rect_min = inset;
    let rect_max = size - inset;
    let rect_size = rect_max - rect_min;
    let radius = size * 0.24;
    let distance = rounded_rect_distance(x, y, rect_min, rect_min, rect_size, rect_size, radius);
    let edge_alpha = smoothstep(1.0, -1.0, distance);
    if edge_alpha <= 0.0 {
        return [0.0, 0.0, 0.0, 0.0];
    }

    let t = ((y - rect_min) / rect_size).clamp(0.0, 1.0);
    let mut rgb = mix_rgb(palette.top, palette.bottom, t);

    let top_center = [size * 0.5, rect_min + size * 0.08];
    let top_glow = (1.0 - distance_to(x, y, top_center) / (size * 0.5)).clamp(0.0, 1.0) * 0.04;
    rgb = mix_rgb(rgb, [1.0, 1.0, 1.0], top_glow);

    let bottom_center = [size * 0.5, rect_max];
    let bottom_shade =
        (1.0 - distance_to(x, y, bottom_center) / (size * 0.45)).clamp(0.0, 1.0) * 0.03;
    rgb = mix_rgb(rgb, [0.0, 0.0, 0.0], bottom_shade);

    let cx = size * 0.5;
    let cy = size * 0.5;
    let chevron_h = size * 0.30;
    let chevron_w = size * 0.17;
    let weight = size * 0.09;
    let back_alpha =
        chevron_alpha(x, y, cx + size * -0.10, cy, chevron_w, chevron_h, weight) * 0.40;
    rgb = mix_rgb(rgb, [1.0, 1.0, 1.0], back_alpha);

    let front_alpha = chevron_alpha(x, y, cx + size * 0.10, cy, chevron_w, chevron_h, weight);
    rgb = mix_rgb(rgb, [1.0, 1.0, 1.0], front_alpha);

    let inner_distance = rounded_rect_distance(
        x,
        y,
        rect_min + 0.5,
        rect_min + 0.5,
        rect_size - 1.0,
        rect_size - 1.0,
        radius,
    )
    .abs();
    let border_alpha = (1.0 - inner_distance).clamp(0.0, 1.0) * 0.08;
    rgb = mix_rgb(rgb, [1.0, 1.0, 1.0], border_alpha);

    [rgb[0], rgb[1], rgb[2], edge_alpha]
}

fn icon_palette(style: &str) -> IconPalette {
    match style {
        "cobalt" => IconPalette {
            top: [0.12, 0.14, 0.20],
            bottom: [0.11, 0.13, 0.18],
        },
        "sunset" => IconPalette {
            top: [0.96, 0.42, 0.32],
            bottom: [0.93, 0.38, 0.29],
        },
        "forest" => IconPalette {
            top: [0.18, 0.62, 0.45],
            bottom: [0.16, 0.57, 0.42],
        },
        _ => IconPalette {
            top: [0.24, 0.50, 0.98],
            bottom: [0.22, 0.45, 0.93],
        },
    }
}

fn chevron_alpha(x: f32, y: f32, cx: f32, cy: f32, width: f32, height: f32, stroke: f32) -> f32 {
    let left_top = [cx - width * 0.5, cy - height * 0.5];
    let center = [cx + width * 0.5, cy];
    let left_bottom = [cx - width * 0.5, cy + height * 0.5];
    let distance = distance_to_segment(x, y, left_top, center).min(distance_to_segment(
        x,
        y,
        center,
        left_bottom,
    ));
    smoothstep(stroke * 0.5 + 1.0, stroke * 0.5 - 1.0, distance)
}

fn rounded_rect_distance(x: f32, y: f32, rx: f32, ry: f32, rw: f32, rh: f32, radius: f32) -> f32 {
    let px = (x - (rx + rw * 0.5)).abs() - (rw * 0.5 - radius);
    let py = (y - (ry + rh * 0.5)).abs() - (rh * 0.5 - radius);
    let outside = [px.max(0.0), py.max(0.0)];
    let inside = px.max(py).min(0.0);
    (outside[0] * outside[0] + outside[1] * outside[1]).sqrt() + inside - radius
}

fn distance_to_segment(x: f32, y: f32, a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [x - a[0], y - a[1]];
    let denom = ab[0] * ab[0] + ab[1] * ab[1];
    if denom <= f32::EPSILON {
        return distance_to(x, y, a);
    }
    let t = ((ap[0] * ab[0] + ap[1] * ab[1]) / denom).clamp(0.0, 1.0);
    distance_to(x, y, [a[0] + ab[0] * t, a[1] + ab[1] * t])
}

fn distance_to(x: f32, y: f32, point: [f32; 2]) -> f32 {
    let dx = x - point[0];
    let dy = y - point[1];
    (dx * dx + dy * dy).sqrt()
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix_rgb(from: [f32; 3], to: [f32; 3], amount: f32) -> [f32; 3] {
    [
        from[0] + (to[0] - from[0]) * amount,
        from[1] + (to[1] - from[1]) * amount,
        from[2] + (to[2] - from[2]) * amount,
    ]
}

fn to_u8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}
