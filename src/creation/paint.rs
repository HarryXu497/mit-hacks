//! Painting on the easel.
//!
//! The canvas is a texture on a slab in the world. A stroke is a ray from the
//! cursor into the scene: where it meets the canvas plane becomes a point in
//! texture space, and pigment is stamped there. There is no 2D surface anywhere
//! in the flow — you are painting an object that exists in the world.
//!
//! Each painting is kept twice. The *layer* is what gets exported: pigment on a
//! transparent ground, the same contract the 2D player-creation screen writes,
//! so whatever consumes those PNGs cannot tell which screen made them. The
//! *display* image is that layer over stretched bark, and is only ever looked at.
//! Strokes are also kept as point lists, which is what makes undo exact and
//! gives the manifest its stroke counts.

use super::scene::{CanvasSurface, CANVAS_H, CANVAS_W};
use super::{CreationPhase, Notice};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Texture resolution of a painting. Matches the 2D screen's 1024x1024 export.
pub const CANVAS_PX: u32 = 1024;

pub const MIN_RADIUS: f32 = 0.004;
pub const MAX_RADIUS: f32 = 0.14;

/// The pigments on the shelf, in dish order. Drawn straight from the stadium's
/// palette so anything painted here already belongs in the jungle.
pub const PIGMENTS: [[f32; 3]; 8] = [
    [0.98, 0.94, 0.74], // warm ivory — the field markings
    [0.05, 0.04, 0.06], // ink — the outline black
    [0.87, 0.36, 0.06], // tribe orange
    [0.08, 0.34, 0.85], // river blue
    [1.00, 0.72, 0.08], // sun gold
    [0.39, 0.65, 0.15], // leaf green
    [0.12, 0.35, 0.19], // canopy deep
    [0.67, 0.41, 0.18], // fur brown
];

/// A dish of ground pigment. The index is into [`PIGMENTS`].
#[derive(Component)]
pub struct PigmentDish(pub usize);

/// The wet rag on the shelf: click it to wipe pigment back off.
#[derive(Component)]
pub struct Rag;

/// Which of the player's two paintings a sheet holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Appearance,
    Superpower,
}

impl Slot {
    /// Filename fragment, shared with the 2D screen's convention.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Appearance => "appearance",
            Self::Superpower => "superpower",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tool {
    Pigment(usize),
    Rag,
}

#[derive(Debug, Clone)]
pub struct Stroke {
    pub tool: Tool,
    pub radius: f32,
    pub points: Vec<Vec2>,
}

/// One painting: its strokes, its exportable layer, and the image on the easel.
pub struct Sheet {
    pub strokes: Vec<Stroke>,
    /// RGBA, straight alpha, transparent where nothing has been painted.
    pub layer: Vec<u8>,
    pub display: Handle<Image>,
}

impl Sheet {
    fn new(images: &mut Assets<Image>) -> Self {
        Self {
            strokes: Vec::new(),
            layer: vec![0; (CANVAS_PX * CANVAS_PX * 4) as usize],
            display: images.add(blank_canvas_image()),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.strokes.is_empty()
    }
}

/// Both paintings. Created by the scene when it builds the easel.
#[derive(Resource)]
pub struct Paintings {
    pub appearance: Sheet,
    pub superpower: Sheet,
}

impl Paintings {
    pub fn new(images: &mut Assets<Image>) -> Self {
        Self {
            appearance: Sheet::new(images),
            superpower: Sheet::new(images),
        }
    }

    pub fn sheet(&self, slot: Slot) -> &Sheet {
        match slot {
            Slot::Appearance => &self.appearance,
            Slot::Superpower => &self.superpower,
        }
    }

    pub fn sheet_mut(&mut self, slot: Slot) -> &mut Sheet {
        match slot {
            Slot::Appearance => &mut self.appearance,
            Slot::Superpower => &mut self.superpower,
        }
    }
}

/// What the brush is loaded with, and how wide it draws. Shared across both
/// paintings so moving to the superpower canvas keeps the brush you had.
#[derive(Resource)]
pub struct Brush {
    pub tool: Tool,
    /// The last pigment picked, so B returns to it after using the rag.
    pub pigment: usize,
    /// Radius as a fraction of canvas width, so it is resolution-independent.
    pub radius: f32,
}

impl Default for Brush {
    fn default() -> Self {
        // Opens on ink: the first thing anyone draws is an outline.
        Self {
            tool: Tool::Pigment(1),
            pigment: 1,
            radius: 0.018,
        }
    }
}

impl Brush {
    pub fn resize(&mut self, factor: f32) {
        self.radius = (self.radius * factor).clamp(MIN_RADIUS, MAX_RADIUS);
    }
}

/// True while the button is down on a stroke that started on the canvas.
#[derive(Resource, Default)]
pub struct StrokeState {
    drawing: bool,
}

/// The bark ground at one pixel: pale, faintly uneven, never flat paper.
fn bark(x: usize, y: usize) -> [u8; 3] {
    let fx = x as f32 / CANVAS_PX as f32;
    let fy = y as f32 / CANVAS_PX as f32;
    let grain = ((fx * 41.0).sin() * (fy * 37.0).cos()) * 0.018
        + ((fx * 7.0 + fy * 5.0).sin()) * 0.012;
    let base = [0.93_f32, 0.88, 0.71];
    [0, 1, 2].map(|ch| ((base[ch] + grain).clamp(0., 1.) * 255.0) as u8)
}

fn blank_display() -> Vec<u8> {
    let size = CANVAS_PX as usize;
    let mut data = vec![255u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let i = (y * size + x) * 4;
            data[i..i + 3].copy_from_slice(&bark(x, y));
        }
    }
    data
}

/// A fresh canvas image: bare stretched bark.
pub fn blank_canvas_image() -> Image {
    Image::new(
        Extent3d {
            width: CANVAS_PX,
            height: CANVAS_PX,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        blank_display(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    )
}

/// Where a ray meets the canvas, in 0..1 texture space. `None` if it misses.
///
/// Kept as a free function of plain geometry so the mapping can be tested
/// without a window, a camera or a running app.
pub fn canvas_hit(origin: Vec3, dir: Vec3, centre: Vec3, rotation: Quat) -> Option<Vec2> {
    let normal = rotation * Vec3::Z;
    let denom = dir.dot(normal);
    // Facing away, or parallel to the plane: no usable intersection.
    if denom.abs() < 1e-5 {
        return None;
    }
    let t = (centre - origin).dot(normal) / denom;
    if t <= 0.0 {
        return None;
    }
    let local = rotation.inverse() * (origin + dir * t - centre);
    let u = local.x / CANVAS_W + 0.5;
    // Texture rows run downward, world Y runs up.
    let v = 0.5 - local.y / CANVAS_H;
    if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
        return None;
    }
    Some(Vec2::new(u, v))
}

/// One round dab, written to the export layer and the display image together.
fn stamp(layer: &mut [u8], display: &mut [u8], at: Vec2, radius: f32, tool: Tool) {
    let size = CANVAS_PX as i32;
    let r_px = (radius * CANVAS_PX as f32).max(1.0);
    let cx = at.x * CANVAS_PX as f32;
    let cy = at.y * CANVAS_PX as f32;
    let lo_x = ((cx - r_px).floor() as i32).max(0);
    let hi_x = ((cx + r_px).ceil() as i32).min(size - 1);
    let lo_y = ((cy - r_px).floor() as i32).max(0);
    let hi_y = ((cy + r_px).ceil() as i32).min(size - 1);
    for y in lo_y..=hi_y {
        for x in lo_x..=hi_x {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            if d > r_px {
                continue;
            }
            // Soft only in the outermost pixel: the look is poster-flat, so a
            // wide feather would fight the rest of the style.
            let a = ((r_px - d) / 1.5).clamp(0., 1.);
            let i = ((y * size + x) as usize) * 4;
            match tool {
                Tool::Pigment(index) => {
                    let rgb = PIGMENTS[index];
                    // Straight-alpha "over" into the layer.
                    let dst_a = layer[i + 3] as f32 / 255.0;
                    let out_a = a + dst_a * (1.0 - a);
                    for ch in 0..3 {
                        let dst = layer[i + ch] as f32 / 255.0;
                        let mixed = (rgb[ch] * a + dst * dst_a * (1.0 - a)) / out_a.max(1e-5);
                        layer[i + ch] = (mixed.clamp(0., 1.) * 255.0) as u8;
                    }
                    layer[i + 3] = (out_a.clamp(0., 1.) * 255.0) as u8;
                }
                Tool::Rag => {
                    let dst_a = layer[i + 3] as f32 / 255.0;
                    layer[i + 3] = ((dst_a * (1.0 - a)).clamp(0., 1.) * 255.0) as u8;
                }
            }
            // The display is always "layer over bark", recomputed for the pixel.
            let ground = bark(x as usize, y as usize);
            let la = layer[i + 3] as f32 / 255.0;
            for ch in 0..3 {
                let over = layer[i + ch] as f32 * la + ground[ch] as f32 * (1.0 - la);
                display[i + ch] = over.clamp(0., 255.) as u8;
            }
        }
    }
}

/// Dabs from `from` to `to`, close enough together to read as one line.
fn draw_segment(layer: &mut [u8], display: &mut [u8], from: Vec2, to: Vec2, radius: f32, tool: Tool) {
    let span = (to - from).length();
    let steps = (span / (radius * 0.4).max(1e-4)).ceil().clamp(1.0, 96.0) as u32;
    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        stamp(layer, display, from.lerp(to, t), radius, tool);
    }
}

fn draw_stroke(layer: &mut [u8], display: &mut [u8], stroke: &Stroke) {
    let Some(first) = stroke.points.first() else {
        return;
    };
    stamp(layer, display, *first, stroke.radius, stroke.tool);
    for pair in stroke.points.windows(2) {
        draw_segment(layer, display, pair[0], pair[1], stroke.radius, stroke.tool);
    }
}

/// Rebuilds a sheet's pixels from its strokes. Used by undo and clear, where
/// replaying is the only way to get back exactly what was there before.
fn repaint(sheet: &mut Sheet, images: &mut Assets<Image>) {
    sheet.layer.fill(0);
    let mut display = blank_display();
    for stroke in &sheet.strokes {
        draw_stroke(&mut sheet.layer, &mut display, stroke);
    }
    if let Some(image) = images.get_mut(&sheet.display) {
        image.data = display;
    }
}

/// Paints while the left button is held; picks pigment or the rag from the shelf.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn paint(
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<super::camera::CreationCamera>>,
    canvases: Query<(&Transform, &Handle<StandardMaterial>), With<CanvasSurface>>,
    shelf: Query<(&GlobalTransform, Option<&PigmentDish>), Or<(With<PigmentDish>, With<Rag>)>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut paintings: Option<ResMut<Paintings>>,
    mut brush: ResMut<Brush>,
    mut stroke: ResMut<StrokeState>,
    buttons: Res<ButtonInput<MouseButton>>,
    phase: Res<State<CreationPhase>>,
) {
    let (Some(paintings), Some(slot)) = (paintings.as_mut(), phase.get().slot()) else {
        return;
    };
    if !buttons.pressed(MouseButton::Left) {
        stroke.drawing = false;
        return;
    }
    let Ok(window) = windows.get_single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok((camera, camera_tf)) = cameras.get_single() else {
        return;
    };
    let Some(ray) = camera.viewport_to_world(camera_tf, cursor) else {
        return;
    };

    // A click near the shelf loads the brush instead of painting.
    if buttons.just_pressed(MouseButton::Left) {
        let mut best: Option<(f32, Tool)> = None;
        for (tf, dish) in &shelf {
            let to = tf.translation() - ray.origin;
            let along = to.dot(*ray.direction);
            if along <= 0.0 {
                continue;
            }
            // Nearest to the ray wins, not nearest to the camera: the dishes sit
            // close together, and a ray aimed at a back dish passes low over the
            // front ones, so picking by depth chose the wrong colour.
            let miss = (to - *ray.direction * along).length();
            if miss < 0.27 && best.map_or(true, |(b, _)| miss < b) {
                best = Some((miss, dish.map_or(Tool::Rag, |d| Tool::Pigment(d.0))));
            }
        }
        if let Some((_, tool)) = best {
            brush.tool = tool;
            if let Tool::Pigment(index) = tool {
                brush.pigment = index;
            }
            stroke.drawing = false;
            return;
        }
    }

    let Ok((canvas_tf, material)) = canvases.get_single() else {
        return;
    };
    // The canvas's own rotation, so the clearing can be turned to any heading
    // without the hit test and the geometry drifting apart.
    let Some(hit) = canvas_hit(ray.origin, *ray.direction, canvas_tf.translation, canvas_tf.rotation)
    else {
        // Leaving the canvas ends the stroke; coming back starts a new one.
        stroke.drawing = false;
        return;
    };

    let sheet = paintings.sheet_mut(slot);
    let Some(image) = images.get_mut(&sheet.display) else {
        return;
    };
    if stroke.drawing {
        let active = sheet.strokes.last_mut().expect("a stroke is open while drawing");
        let previous = *active.points.last().expect("an open stroke has a point");
        // Skip sub-pixel jitter so held-still frames do not bloat the stroke.
        if (hit - previous).length() * (CANVAS_PX as f32) < 0.75 {
            return;
        }
        let (radius, tool) = (active.radius, active.tool);
        active.points.push(hit);
        draw_segment(&mut sheet.layer, &mut image.data, previous, hit, radius, tool);
    } else {
        sheet.strokes.push(Stroke {
            tool: brush.tool,
            radius: brush.radius,
            points: vec![hit],
        });
        stamp(&mut sheet.layer, &mut image.data, hit, brush.radius, brush.tool);
        stroke.drawing = true;
    }
    // Changing an image's pixels does not, on its own, reach the screen: the
    // material keeps the texture binding it was built with. Touching the
    // material is what makes the renderer pick up the repainted canvas.
    let _ = mats.get_mut(material);
}

/// Keyboard and wheel controls, matching the 2D screen's shortcuts: Ctrl+Z
/// undo, B brush, E eraser, [ and ] for size. Delete clears the canvas.
#[allow(clippy::too_many_arguments)]
pub fn tools(
    keys: Res<ButtonInput<KeyCode>>,
    mut wheel: EventReader<bevy::input::mouse::MouseWheel>,
    mut brush: ResMut<Brush>,
    mut paintings: Option<ResMut<Paintings>>,
    mut images: ResMut<Assets<Image>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    canvases: Query<&Handle<StandardMaterial>, With<CanvasSurface>>,
    phase: Res<State<CreationPhase>>,
    mut stroke: ResMut<StrokeState>,
    mut notice: ResMut<Notice>,
) {
    for event in wheel.read() {
        brush.resize(if event.y > 0. { 1.18 } else { 1.0 / 1.18 });
    }
    if keys.just_pressed(KeyCode::BracketRight) {
        brush.resize(1.25);
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        brush.resize(0.8);
    }
    if keys.just_pressed(KeyCode::KeyB) {
        brush.tool = Tool::Pigment(brush.pigment);
    }
    if keys.just_pressed(KeyCode::KeyE) {
        brush.tool = Tool::Rag;
    }

    let (Some(paintings), Some(slot)) = (paintings.as_mut(), phase.get().slot()) else {
        return;
    };
    let control = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let undo = control && keys.just_pressed(KeyCode::KeyZ);
    let clear = keys.just_pressed(KeyCode::Delete);
    if !undo && !clear {
        return;
    }
    let sheet = paintings.sheet_mut(slot);
    if sheet.strokes.is_empty() {
        return;
    }
    if undo {
        sheet.strokes.pop();
    } else {
        sheet.strokes.clear();
    }
    stroke.drawing = false;
    notice.clear();
    repaint(sheet, &mut images);
    if let Ok(material) = canvases.get_single() {
        let _ = mats.get_mut(material);
    }
}

/// Puts the right painting on the easel when the phase changes.
pub fn show_active_sheet(
    phase: Res<State<CreationPhase>>,
    paintings: Option<Res<Paintings>>,
    canvases: Query<&Handle<StandardMaterial>, With<CanvasSurface>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    if !phase.is_changed() {
        return;
    }
    let (Some(paintings), Some(slot)) = (paintings, phase.get().easel_slot()) else {
        return;
    };
    if let Ok(handle) = canvases.get_single() {
        if let Some(material) = mats.get_mut(handle) {
            material.base_color_texture = Some(paintings.sheet(slot).display.clone());
        }
    }
}

/// PNG bytes of a sheet's export layer: pigment on a transparent ground.
pub fn encode_png(sheet: &Sheet) -> Result<Vec<u8>, String> {
    let buffer = image::RgbaImage::from_raw(CANVAS_PX, CANVAS_PX, sheet.layer.clone())
        .ok_or("the painting's pixel buffer does not match its size")?;
    let mut encoded = Vec::new();
    image::DynamicImage::ImageRgba8(buffer)
        .write_to(&mut std::io::Cursor::new(&mut encoded), image::ImageOutputFormat::Png)
        .map_err(|error| format!("encoding the painting: {error}"))?;
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffers() -> (Vec<u8>, Vec<u8>) {
        (vec![0; (CANVAS_PX * CANVAS_PX * 4) as usize], blank_display())
    }

    fn middle() -> usize {
        ((CANVAS_PX / 2 * CANVAS_PX + CANVAS_PX / 2) * 4) as usize
    }

    #[test]
    fn a_ray_down_the_middle_lands_in_the_centre_of_the_canvas() {
        let hit = canvas_hit(Vec3::new(0., 0., 5.), Vec3::NEG_Z, Vec3::ZERO, Quat::IDENTITY)
            .expect("a ray straight at the canvas must hit it");
        assert!((hit.x - 0.5).abs() < 1e-4, "horizontal centre");
        assert!((hit.y - 0.5).abs() < 1e-4, "vertical centre");
    }

    #[test]
    fn texture_rows_run_downward_while_world_y_runs_up() {
        let above = canvas_hit(Vec3::new(0., 1., 5.), Vec3::NEG_Z, Vec3::ZERO, Quat::IDENTITY).unwrap();
        let below = canvas_hit(Vec3::new(0., -1., 5.), Vec3::NEG_Z, Vec3::ZERO, Quat::IDENTITY).unwrap();
        assert!(above.y < below.y, "higher in the world is a smaller texture row");
    }

    #[test]
    fn rays_that_miss_the_board_report_no_hit() {
        assert!(canvas_hit(Vec3::new(CANVAS_W, 0., 5.), Vec3::NEG_Z, Vec3::ZERO, Quat::IDENTITY).is_none());
        assert!(canvas_hit(Vec3::new(0., 0., 5.), Vec3::Z, Vec3::ZERO, Quat::IDENTITY).is_none());
    }

    #[test]
    fn an_unpainted_layer_is_fully_transparent_and_the_display_fully_opaque() {
        let (layer, display) = buffers();
        assert!(layer.chunks(4).all(|p| p[3] == 0), "nothing painted, nothing exported");
        assert!(display.chunks(4).all(|p| p[3] == 255), "the bark ground is opaque");
    }

    #[test]
    fn pigment_lands_in_both_the_export_layer_and_the_display() {
        let (mut layer, mut display) = buffers();
        let corner = display[0];
        stamp(&mut layer, &mut display, Vec2::splat(0.5), 0.05, Tool::Pigment(1));
        let i = middle();
        assert_eq!(layer[i + 3], 255, "the export layer is opaque where painted");
        assert!(display[i] < 40, "and the easel shows the ink");
        assert_eq!(layer[3], 0, "the far corner of the layer is still transparent");
        assert_eq!(display[0], corner, "and the far corner of the display untouched");
    }

    #[test]
    fn the_rag_wipes_back_to_transparent_and_bare_bark() {
        let (mut layer, mut display) = buffers();
        let bare = display[middle()];
        stamp(&mut layer, &mut display, Vec2::splat(0.5), 0.05, Tool::Pigment(1));
        stamp(&mut layer, &mut display, Vec2::splat(0.5), 0.06, Tool::Rag);
        let i = middle();
        assert_eq!(layer[i + 3], 0, "wiped pigment does not export");
        assert_eq!(display[i], bare, "and the bark shows through again");
    }

    #[test]
    fn dropping_a_stroke_and_replaying_matches_never_having_drawn_it() {
        // Undo depends on this.
        let keep = Stroke {
            tool: Tool::Pigment(2),
            radius: 0.03,
            points: vec![Vec2::new(0.2, 0.2), Vec2::new(0.8, 0.7)],
        };
        let dropped = Stroke {
            tool: Tool::Pigment(3),
            radius: 0.05,
            points: vec![Vec2::new(0.5, 0.1), Vec2::new(0.5, 0.9)],
        };
        let (mut never_layer, mut never_display) = buffers();
        draw_stroke(&mut never_layer, &mut never_display, &keep);

        let mut strokes = vec![keep.clone(), dropped];
        strokes.pop();
        let (mut undone_layer, mut undone_display) = buffers();
        for stroke in &strokes {
            draw_stroke(&mut undone_layer, &mut undone_display, stroke);
        }
        assert!(never_layer == undone_layer && never_display == undone_display);
    }

    #[test]
    fn brush_size_stays_within_its_limits() {
        let mut brush = Brush::default();
        for _ in 0..40 {
            brush.resize(1.25);
        }
        assert_eq!(brush.radius, MAX_RADIUS);
        for _ in 0..80 {
            brush.resize(0.8);
        }
        assert_eq!(brush.radius, MIN_RADIUS);
    }
}
