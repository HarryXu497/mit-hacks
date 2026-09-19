//! Drawing data model and rasterizer.
//!
//! Strokes are the source of truth. The RGBA raster is a derived cache, and the
//! exported PNG is an artifact produced from that cache. Nothing here depends on
//! Bevy or egui, so the whole model is testable without a window or a GPU.

use anyhow::{Context, Result};

/// Normalized `0..1` canvas coordinate, matching the field-coordinate
/// convention used elsewhere in the repository. Keeping strokes normalized is
/// what makes a drawing survive a window resize unchanged.
pub type CanvasPoint = [f32; 2];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrushTool {
    Brush,
    Eraser,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub points: Vec<CanvasPoint>,
    /// RGBA. Ignored for `BrushTool::Eraser`, which clears to transparent.
    pub color: [u8; 4],
    /// Stroke width as a fraction of the canvas width.
    pub width: f32,
    pub tool: BrushTool,
}

impl Stroke {
    fn radius_pixels(&self, canvas_width: u32) -> f32 {
        (self.width * canvas_width as f32 * 0.5).max(0.5)
    }
}

pub struct Canvas {
    width: u32,
    height: u32,
    strokes: Vec<Stroke>,
    /// Stroke currently being drawn; not yet committed to `strokes`.
    active: Option<Stroke>,
    /// Derived RGBA8 cache of the committed strokes.
    pixels: Vec<u8>,
    dirty: bool,
}

impl std::fmt::Debug for Canvas {
    // The pixel buffer is megabytes; never print it.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Canvas")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("strokes", &self.strokes.len())
            .field("drawing", &self.active.is_some())
            .finish()
    }
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            strokes: Vec::new(),
            active: None,
            pixels: vec![0; width as usize * height as usize * 4],
            dirty: false,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn stroke_count(&self) -> usize {
        self.strokes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strokes.is_empty() && self.active.is_none()
    }

    /// The in-progress stroke, so the UI can preview it without rasterizing.
    pub fn active_stroke(&self) -> Option<&Stroke> {
        self.active.as_ref()
    }

    pub fn strokes(&self) -> &[Stroke] {
        &self.strokes
    }

    pub fn begin_stroke(&mut self, tool: BrushTool, color: [u8; 4], width: f32) {
        self.active = Some(Stroke {
            points: Vec::new(),
            color,
            width,
            tool,
        });
    }

    /// Appends a point, skipping samples too close to the previous one so a
    /// slow drag does not accumulate thousands of redundant points.
    pub fn extend_stroke(&mut self, point: CanvasPoint) {
        let point = [point[0].clamp(0.0, 1.0), point[1].clamp(0.0, 1.0)];
        let Some(active) = &mut self.active else {
            return;
        };
        let far_enough = active.points.last().is_none_or(|previous| {
            let dx = previous[0] - point[0];
            let dy = previous[1] - point[1];
            (dx * dx + dy * dy).sqrt() >= 0.002
        });
        if far_enough {
            active.points.push(point);
        }
    }

    /// Commits the in-progress stroke. A stroke with no points is discarded; a
    /// single-point stroke is kept so that a click leaves a dot.
    pub fn end_stroke(&mut self) {
        let Some(active) = self.active.take() else {
            return;
        };
        if active.points.is_empty() {
            return;
        }
        rasterize_stroke(&mut self.pixels, self.width, self.height, &active);
        self.strokes.push(active);
    }

    /// Drops the in-progress stroke without committing it.
    pub fn cancel_stroke(&mut self) {
        self.active = None;
    }

    /// Removes exactly one committed stroke and forces a full re-raster.
    pub fn undo(&mut self) -> bool {
        self.active = None;
        if self.strokes.pop().is_none() {
            return false;
        }
        self.dirty = true;
        true
    }

    /// Clears this canvas only.
    pub fn clear(&mut self) {
        self.active = None;
        if self.strokes.is_empty() {
            return;
        }
        self.strokes.clear();
        self.dirty = true;
    }

    /// Committed pixels as RGBA8, re-rasterizing only when the stroke list
    /// changed in a way that cannot be applied incrementally.
    pub fn raster(&mut self) -> &[u8] {
        if self.dirty {
            self.pixels.fill(0);
            for stroke in &self.strokes {
                rasterize_stroke(&mut self.pixels, self.width, self.height, stroke);
            }
            self.dirty = false;
        }
        &self.pixels
    }

    pub fn needs_raster(&self) -> bool {
        self.dirty
    }

    /// PNG-encodes the committed pixels. The exported artifact is always
    /// derived from the same rasterizer the preview uses.
    pub fn to_png(&mut self) -> Result<Vec<u8>> {
        let width = self.width;
        let height = self.height;
        let pixels = self.raster().to_vec();
        let buffer = image::RgbaImage::from_raw(width, height, pixels)
            .context("canvas pixel buffer does not match its dimensions")?;
        let mut encoded = Vec::new();
        image::DynamicImage::ImageRgba8(buffer)
            .write_to(
                &mut std::io::Cursor::new(&mut encoded),
                image::ImageOutputFormat::Png,
            )
            .context("encoding canvas as PNG")?;
        Ok(encoded)
    }
}

/// Stamps a stroke into an RGBA8 buffer as round-capped segments.
fn rasterize_stroke(pixels: &mut [u8], width: u32, height: u32, stroke: &Stroke) {
    let radius = stroke.radius_pixels(width);
    let to_pixels = |point: CanvasPoint| {
        [
            point[0] * (width.saturating_sub(1)) as f32,
            point[1] * (height.saturating_sub(1)) as f32,
        ]
    };

    if stroke.points.len() == 1 {
        stamp_disc(
            pixels,
            width,
            height,
            to_pixels(stroke.points[0]),
            radius,
            stroke,
        );
        return;
    }
    for pair in stroke.points.windows(2) {
        stamp_segment(
            pixels,
            width,
            height,
            to_pixels(pair[0]),
            to_pixels(pair[1]),
            radius,
            stroke,
        );
    }
}

fn stamp_disc(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    center: [f32; 2],
    radius: f32,
    stroke: &Stroke,
) {
    stamp_segment(pixels, width, height, center, center, radius, stroke);
}

/// Fills every pixel within `radius` of the segment `a..b`.
fn stamp_segment(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    a: [f32; 2],
    b: [f32; 2],
    radius: f32,
    stroke: &Stroke,
) {
    let min_x = (a[0].min(b[0]) - radius).floor().max(0.0) as u32;
    let max_x = (a[0].max(b[0]) + radius).ceil().min((width - 1) as f32) as u32;
    let min_y = (a[1].min(b[1]) - radius).floor().max(0.0) as u32;
    let max_y = (a[1].max(b[1]) + radius).ceil().min((height - 1) as f32) as u32;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let distance = distance_to_segment([x as f32, y as f32], a, b);
            if distance > radius {
                continue;
            }
            // One pixel of feathering keeps edges from looking stair-stepped.
            let coverage = (radius - distance).clamp(0.0, 1.0);
            let index = ((y as usize * width as usize) + x as usize) * 4;
            match stroke.tool {
                BrushTool::Brush => blend(&mut pixels[index..index + 4], stroke.color, coverage),
                BrushTool::Eraser => erase(&mut pixels[index..index + 4], coverage),
            }
        }
    }
}

fn blend(target: &mut [u8], color: [u8; 4], coverage: f32) {
    let source_alpha = (color[3] as f32 / 255.0) * coverage;
    if source_alpha <= 0.0 {
        return;
    }
    let destination_alpha = target[3] as f32 / 255.0;
    let out_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    if out_alpha <= 0.0 {
        target.fill(0);
        return;
    }
    for channel in 0..3 {
        let source = color[channel] as f32 / 255.0;
        let destination = target[channel] as f32 / 255.0;
        let value = (source * source_alpha
            + destination * destination_alpha * (1.0 - source_alpha))
            / out_alpha;
        target[channel] = (value * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    target[3] = (out_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// The eraser reduces alpha, so exported PNGs stay transparent rather than
/// picking up an opaque background colour.
fn erase(target: &mut [u8], coverage: f32) {
    let remaining = (target[3] as f32 / 255.0) * (1.0 - coverage);
    target[3] = (remaining * 255.0).round().clamp(0.0, 255.0) as u8;
    if target[3] == 0 {
        target.fill(0);
    }
}

fn distance_to_segment(point: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (abx, aby) = (b[0] - a[0], b[1] - a[1]);
    let length_squared = abx * abx + aby * aby;
    let (dx, dy) = if length_squared == 0.0 {
        (point[0] - a[0], point[1] - a[1])
    } else {
        let t =
            (((point[0] - a[0]) * abx + (point[1] - a[1]) * aby) / length_squared).clamp(0.0, 1.0);
        (point[0] - (a[0] + t * abx), point[1] - (a[1] + t * aby))
    };
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: [u8; 4] = [255, 255, 255, 255];

    fn canvas_with_strokes(count: usize) -> Canvas {
        let mut canvas = Canvas::new(64, 64);
        for index in 0..count {
            let offset = index as f32 * 0.1;
            canvas.begin_stroke(BrushTool::Brush, WHITE, 0.1);
            canvas.extend_stroke([0.1 + offset, 0.5]);
            canvas.extend_stroke([0.3 + offset, 0.5]);
            canvas.end_stroke();
        }
        canvas
    }

    fn alpha_at(canvas: &mut Canvas, x: u32, y: u32) -> u8 {
        let width = canvas.width();
        let pixels = canvas.raster();
        pixels[((y as usize * width as usize) + x as usize) * 4 + 3]
    }

    #[test]
    fn undo_pops_exactly_one_stroke() {
        let mut canvas = canvas_with_strokes(3);
        assert!(canvas.undo());
        assert_eq!(canvas.stroke_count(), 2);
        assert!(canvas.undo());
        assert!(canvas.undo());
        assert_eq!(canvas.stroke_count(), 0);
        assert!(
            !canvas.undo(),
            "undo on an empty canvas reports nothing to do"
        );
    }

    #[test]
    fn undo_repaints_the_raster() {
        let mut canvas = Canvas::new(64, 64);
        canvas.begin_stroke(BrushTool::Brush, WHITE, 0.2);
        canvas.extend_stroke([0.5, 0.5]);
        canvas.end_stroke();
        assert!(alpha_at(&mut canvas, 32, 32) > 0);

        canvas.undo();
        assert_eq!(
            alpha_at(&mut canvas, 32, 32),
            0,
            "undone strokes must disappear from the exported pixels"
        );
    }

    #[test]
    fn clear_removes_every_stroke_but_keeps_the_canvas_usable() {
        let mut canvas = canvas_with_strokes(2);
        canvas.clear();
        assert!(canvas.is_empty());
        assert!(canvas.raster().iter().all(|byte| *byte == 0));

        canvas.begin_stroke(BrushTool::Brush, WHITE, 0.2);
        canvas.extend_stroke([0.5, 0.5]);
        canvas.end_stroke();
        assert_eq!(canvas.stroke_count(), 1);
    }

    #[test]
    fn eraser_lowers_alpha_without_adding_colour() {
        let mut canvas = Canvas::new(64, 64);
        canvas.begin_stroke(BrushTool::Brush, WHITE, 0.4);
        canvas.extend_stroke([0.5, 0.5]);
        canvas.end_stroke();
        assert!(alpha_at(&mut canvas, 32, 32) > 0);

        canvas.begin_stroke(BrushTool::Eraser, WHITE, 0.4);
        canvas.extend_stroke([0.5, 0.5]);
        canvas.end_stroke();
        assert_eq!(alpha_at(&mut canvas, 32, 32), 0);
    }

    #[test]
    fn an_in_progress_stroke_is_not_committed_until_it_ends() {
        let mut canvas = Canvas::new(64, 64);
        canvas.begin_stroke(BrushTool::Brush, WHITE, 0.2);
        canvas.extend_stroke([0.5, 0.5]);
        assert_eq!(canvas.stroke_count(), 0);
        assert!(canvas.active_stroke().is_some());
        assert!(
            !canvas.is_empty(),
            "a stroke in progress still counts as content"
        );

        canvas.end_stroke();
        assert_eq!(canvas.stroke_count(), 1);
        assert!(canvas.active_stroke().is_none());
    }

    #[test]
    fn cancelled_and_empty_strokes_are_discarded() {
        let mut canvas = Canvas::new(64, 64);
        canvas.begin_stroke(BrushTool::Brush, WHITE, 0.2);
        canvas.extend_stroke([0.5, 0.5]);
        canvas.cancel_stroke();
        assert!(canvas.is_empty());

        canvas.begin_stroke(BrushTool::Brush, WHITE, 0.2);
        canvas.end_stroke();
        assert_eq!(
            canvas.stroke_count(),
            0,
            "a stroke with no points is dropped"
        );
    }

    #[test]
    fn strokes_stay_normalized_so_resizing_cannot_distort_them() {
        let mut canvas = Canvas::new(64, 64);
        canvas.begin_stroke(BrushTool::Brush, WHITE, 0.1);
        canvas.extend_stroke([-3.0, 7.5]);
        canvas.end_stroke();
        assert_eq!(canvas.strokes()[0].points[0], [0.0, 1.0]);
    }

    #[test]
    fn png_export_round_trips_at_canvas_resolution() {
        let mut canvas = canvas_with_strokes(1);
        let encoded = canvas.to_png().unwrap();
        assert_eq!(&encoded[1..4], b"PNG");

        let decoded = image::load_from_memory(&encoded).unwrap();
        assert_eq!(decoded.width(), 64);
        assert_eq!(decoded.height(), 64);
    }
}
