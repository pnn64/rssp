use png;
use std::fs::File;

#[derive(Debug, Clone, Copy)]
pub enum ColorScheme {
    Default,
    Alternative,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphImageData {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

fn generate_graph_pixels(
    measure_nps_vec: &[f64],
    max_nps: f64,
    width: u32,
    height: u32,
    bottom_color: [u8; 3],
    top_color: [u8; 3],
    bg_color: [u8; 3],
) -> Vec<u8> {
    let color_gradient: Vec<[u8; 3]> = (0..height)
        .map(|y| {
            let frac = f64::from(height - 1 - y) / (f64::from(height) - 1.0);
            let r = (f64::from(bottom_color[0]) + (f64::from(top_color[0]) - f64::from(bottom_color[0])) * frac)
                .round() as u8;
            let g = (f64::from(bottom_color[1]) + (f64::from(top_color[1]) - f64::from(bottom_color[1])) * frac)
                .round() as u8;
            let b = (f64::from(bottom_color[2]) + (f64::from(top_color[2]) - f64::from(bottom_color[2])) * frac)
                .round() as u8;
            [r, g, b]
        })
        .collect();

    let mut img_buffer = vec![0; (width * height * 3) as usize];
    img_buffer
        .chunks_exact_mut(3)
        .for_each(|pixel| pixel.copy_from_slice(&bg_color));

    if !measure_nps_vec.is_empty() && max_nps > 0.0 {
        let num_measures = measure_nps_vec.len();
        let measure_width = f64::from(width) / num_measures as f64;

        let h_vec: Vec<f64> = measure_nps_vec
            .iter()
            .map(|&nps| (nps / max_nps).min(1.0) * f64::from(height))
            .collect();

        for x in 0..width {
            let x_f = f64::from(x);
            let i = (x_f / measure_width).floor() as usize;
            if i >= num_measures {
                continue;
            }

            let frac = (i as f64).mul_add(-measure_width, x_f) / measure_width;

            let h_start = h_vec[i];
            let h_end = if i < num_measures - 1 {
                h_vec[i + 1]
            } else {
                h_start
            };
            let h_x = frac.mul_add(h_end - h_start, h_start);
            let bar_height = h_x.round() as u32;

            if bar_height == 0 {
                continue;
            }

            let y_top = height.saturating_sub(bar_height);
            for y in y_top..height {
                let color = color_gradient[y as usize];
                let idx = (y * width + x) as usize * 3;
                img_buffer[idx..idx + 3].copy_from_slice(&color);
            }
        }
    }
    img_buffer
}

pub fn generate_density_graph_png(
    measure_nps_vec: &[f64],
    max_nps: f64,
    short_hash: &str,
    color_scheme: &ColorScheme,
) -> std::io::Result<()> {
    const IMAGE_WIDTH: u32 = 1000;
    const GRAPH_HEIGHT: u32 = 400;

    let (bottom_color, top_color, bg_color) = match color_scheme {
        ColorScheme::Default => ([0, 184, 204], [130, 0, 161], [30, 40, 47]), // Cyan to Purple
        ColorScheme::Alternative => ([247, 243, 51], [236, 122, 25], [30, 40, 47]), // Yellow to Orange
    };
    let img_buffer_rgb = generate_graph_pixels(
        measure_nps_vec,
        max_nps,
        IMAGE_WIDTH,
        GRAPH_HEIGHT,
        bottom_color,
        top_color,
        bg_color,
    );

    let filename = match color_scheme {
        ColorScheme::Default => format!("{short_hash}.png"),
        ColorScheme::Alternative => format!("{short_hash}-alt.png"),
    };
    let file = File::create(filename)?;
    let mut encoder = png::Encoder::new(file, IMAGE_WIDTH, GRAPH_HEIGHT);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&img_buffer_rgb)?;

    Ok(())
}

pub fn generate_density_graph_rgba_data(
    measure_nps_vec: &[f64],
    max_nps: f64,
    width: u32,
    height: u32,
    bottom_color: [u8; 3],
    top_color: [u8; 3],
    bg_color: [u8; 3],
) -> Result<GraphImageData, String> {
    let rgb_data = generate_graph_pixels(
        measure_nps_vec,
        max_nps,
        width,
        height,
        bottom_color,
        top_color,
        bg_color,
    );

    let rgba_data = rgb_data
        .chunks_exact(3)
        .flat_map(|rgb| [rgb[0], rgb[1], rgb[2], 255])
        .collect();

    Ok(GraphImageData {
        width,
        height,
        data: rgba_data,
    })
}
