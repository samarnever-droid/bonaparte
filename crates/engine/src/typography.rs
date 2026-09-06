//! Pure-Rust, bundled-font typography. No OS font lookup or host dependencies.
use fontdue::{Font, FontSettings};
use std::sync::OnceLock;

fn font(bold: bool) -> &'static Font {
    static REGULAR: OnceLock<Font> = OnceLock::new();
    static BOLD: OnceLock<Font> = OnceLock::new();
    if bold {
        BOLD.get_or_init(|| {
            Font::from_bytes(
                include_bytes!("../assets/DejaVuSans-Bold.ttf") as &[u8],
                FontSettings::default(),
            )
            .expect("bundled font")
        })
    } else {
        REGULAR.get_or_init(|| {
            Font::from_bytes(
                include_bytes!("../assets/DejaVuSans.ttf") as &[u8],
                FontSettings::default(),
            )
            .expect("bundled font")
        })
    }
}

pub struct TextBitmap {
    pub width: u32,
    pub height: u32,
    pub coverage: Vec<u8>,
}
impl TextBitmap {
    pub fn sample(&self, u: f32, v: f32) -> f32 {
        let x = (u * self.width as f32).floor() as u32;
        let y = (v * self.height as f32).floor() as u32;
        if x >= self.width || y >= self.height {
            return 0.0;
        }
        self.coverage[(y * self.width + x) as usize] as f32 / 255.0
    }
}

pub fn measure_text(text: &str, size: f32, bold: bool, tracking: f32) -> [f32; 2] {
    let f = font(bold);
    let line_height = f
        .horizontal_line_metrics(size)
        .map(|m| m.new_line_size)
        .unwrap_or(size * 1.2);
    let mut max_width = 1.0f32;
    let mut line_count = 0;
    for line in text.split('\n') {
        line_count += 1;
        let mut advance = 0.0;
        let mut previous = None;
        for ch in line.chars() {
            if let Some(prev) = previous {
                advance += f.horizontal_kern(prev, ch, size).unwrap_or(0.0) + tracking;
            }
            advance += f.metrics(ch, size).advance_width;
            previous = Some(ch);
        }
        max_width = max_width.max(advance.ceil() + 2.0);
    }
    [max_width, (line_count as f32 * line_height).ceil().max(1.0)]
}

pub fn rasterize_text(
    text: &str,
    size: f32,
    bold: bool,
    tracking: f32,
) -> Result<TextBitmap, String> {
    let [w, h] = measure_text(text, size, bold, tracking);
    let width = w as u32;
    let height = h as u32;
    // Avoid pathological user text allocating enormous temporary buffers.
    if u64::from(width) * u64::from(height) > 16_777_216 {
        return Err(
            "Text layout exceeds 16 megapixels. Reduce font size, tracking or line length.".into(),
        );
    }
    let mut bitmap = TextBitmap {
        width,
        height,
        coverage: vec![0; (width * height) as usize],
    };
    let f = font(bold);
    let metrics = f
        .horizontal_line_metrics(size)
        .expect("bundled font metrics");
    let mut baseline = metrics.ascent;
    for line in text.split('\n') {
        let mut advance = 1.0;
        let mut previous = None;
        for ch in line.chars() {
            if let Some(prev) = previous {
                advance += f.horizontal_kern(prev, ch, size).unwrap_or(0.0) + tracking;
            }
            let (m, mask) = f.rasterize(ch, size);
            let x0 = advance.round() as i32 + m.xmin;
            let y0 = baseline.round() as i32 - m.ymin - m.height as i32;
            for y in 0..m.height {
                for x in 0..m.width {
                    let px = x0 + x as i32;
                    let py = y0 + y as i32;
                    if px >= 0 && py >= 0 && px < width as i32 && py < height as i32 {
                        let i = py as usize * width as usize + px as usize;
                        bitmap.coverage[i] = bitmap.coverage[i].max(mask[y * m.width + x]);
                    }
                }
            }
            advance += m.advance_width;
            previous = Some(ch);
        }
        baseline += metrics.new_line_size;
    }
    Ok(bitmap)
}
