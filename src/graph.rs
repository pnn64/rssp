use png;
use std::fs::File;
use std::io;

pub fn generate_density_graph_png(
    measure_nps_vec: &[f64],
    max_nps: f64,
    short_hash: &str,
) -> io::Result<()> {
    const IMAGE_WIDTH: u32 = 1000;
    const GRAPH_HEIGHT: u32 = 400;

    let bg_color = [3, 17, 44];
    let bottom_color = [0, 184, 204];
    let top_color = [130, 0, 161];

    let mut img_buffer = vec![0u8; (IMAGE_WIDTH * GRAPH_HEIGHT * 3) as usize];

    for y in 0..GRAPH_HEIGHT {
        for x in 0..IMAGE_WIDTH {
            let idx = ((y * IMAGE_WIDTH + x) * 3) as usize;
            img_buffer[idx] = bg_color[0];
            img_buffer[idx + 1] = bg_color[1];
            img_buffer[idx + 2] = bg_color[2];
        }
    }

    if !measure_nps_vec.is_empty() && max_nps > 0.0 {
        let measure_width = IMAGE_WIDTH as f64 / measure_nps_vec.len() as f64;

        for (i, &nps) in measure_nps_vec.iter().enumerate() {
            let x_start = (i as f64 * measure_width).round() as u32;
            let x_end = (((i + 1) as f64) * measure_width).round() as u32;
            let x_end = x_end.min(IMAGE_WIDTH);

            let height_fraction = (nps / max_nps).min(1.0);
            let bar_height = (height_fraction * GRAPH_HEIGHT as f64).round() as u32;

            if bar_height == 0 || x_start >= x_end {
                continue;
            }

            let y_top = GRAPH_HEIGHT.saturating_sub(bar_height);

            for y in y_top..GRAPH_HEIGHT {
                let dist_from_bottom = (GRAPH_HEIGHT - 1 - y) as f64;
                let frac = dist_from_bottom / (GRAPH_HEIGHT as f64 - 1.0);

                let r = ((bottom_color[0] as f64)
                    + (top_color[0] as f64 - bottom_color[0] as f64) * frac)
                    .round() as u8;
                let g = ((bottom_color[1] as f64)
                    + (top_color[1] as f64 - bottom_color[1] as f64) * frac)
                    .round() as u8;
                let b = ((bottom_color[2] as f64)
                    + (top_color[2] as f64 - bottom_color[2] as f64) * frac)
                    .round() as u8;

                let row_start = (y * IMAGE_WIDTH) as usize * 3;
                for x in x_start..x_end {
                    let idx = row_start + (x as usize * 3);
                    img_buffer[idx] = r;
                    img_buffer[idx + 1] = g;
                    img_buffer[idx + 2] = b;
                }
            }
        }
    }

    let filename = format!("{}.png", short_hash);
    let file = File::create(filename)?;

    let mut encoder = png::Encoder::new(file, IMAGE_WIDTH, GRAPH_HEIGHT);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;

    writer.write_image_data(&img_buffer)?;
    Ok(())
}
