use png;
use std::fs::File;
use std::io;

#[derive(Debug, Clone, Copy)]
pub enum ColorScheme {
    Default,
    Alternative,
}

pub fn generate_density_graph_png(
    measure_nps_vec: &[f64],
    max_nps: f64,
    short_hash: &str,
    color_scheme: &ColorScheme,
) -> io::Result<()> {
    const IMAGE_WIDTH: u32 = 1000;
    const GRAPH_HEIGHT: u32 = 400;

    let bg_color = [30, 40, 47];
    let (bottom_color, top_color) = match color_scheme {
        ColorScheme::Default => ([0, 184, 204], [130, 0, 161]),       // Cyan to Purple
        ColorScheme::Alternative => ([247, 243, 51], [236, 122, 25]), // Yellow to Orange
    };

    let color_gradient: Vec<[u8; 3]> = (0..GRAPH_HEIGHT)
        .map(|y| {
            let frac = (GRAPH_HEIGHT - 1 - y) as f64 / (GRAPH_HEIGHT as f64 - 1.0);
            let r = (bottom_color[0] as f64 + (top_color[0] as f64 - bottom_color[0] as f64) * frac).round() as u8;
            let g = (bottom_color[1] as f64 + (top_color[1] as f64 - bottom_color[1] as f64) * frac).round() as u8;
            let b = (bottom_color[2] as f64 + (top_color[2] as f64 - bottom_color[2] as f64) * frac).round() as u8;
            [r, g, b]
        })
        .collect();

    let mut img_buffer = vec![0; (IMAGE_WIDTH * GRAPH_HEIGHT * 3) as usize];
    img_buffer.chunks_exact_mut(3).for_each(|pixel| pixel.copy_from_slice(&bg_color));

    if !measure_nps_vec.is_empty() && max_nps > 0.0 {
        let num_measures = measure_nps_vec.len();
        let measure_width = IMAGE_WIDTH as f64 / num_measures as f64;

        let h_vec: Vec<f64> = measure_nps_vec
            .iter()
            .map(|&nps| (nps / max_nps).min(1.0) * GRAPH_HEIGHT as f64)
            .collect();

        for x in 0..IMAGE_WIDTH {
            let x_f = x as f64;
            let i = (x_f / measure_width).floor() as usize;
            if i >= num_measures {
                continue;
            }

            let frac = (x_f - (i as f64 * measure_width)) / measure_width;

            let h_start = h_vec[i];
            let h_end = if i < num_measures - 1 {
                h_vec[i + 1]
            } else {
                h_start
            };
            let h_x = h_start + frac * (h_end - h_start);
            let bar_height = h_x.round() as u32;

            if bar_height == 0 {
                continue;
            }

            let y_top = GRAPH_HEIGHT.saturating_sub(bar_height);
            for y in y_top..GRAPH_HEIGHT {
                let color = color_gradient[y as usize];
                let idx = (y * IMAGE_WIDTH + x) as usize * 3;
                img_buffer[idx..idx + 3].copy_from_slice(&color);
            }
        }
    }

    let filename = match color_scheme {
        ColorScheme::Default => format!("{}.png", short_hash),
        ColorScheme::Alternative => format!("{}-alt.png", short_hash),
    };
    let file = File::create(filename)?;
    let mut encoder = png::Encoder::new(file, IMAGE_WIDTH, GRAPH_HEIGHT);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&img_buffer)?;

    Ok(())
}