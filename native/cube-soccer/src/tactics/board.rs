//! The board: picking, dragging tokens, and marking them up.
//!
//! The board stands upright on its plinth, portrait, the way a coach's magnetic
//! board does — so a click is a ray and a board position is where that ray meets
//! the board's face. Positions are kept normalised 0..1 across the face, which
//! is the convention the tactical payload uses, so nothing has to be converted
//! on the way out.

use super::scene::BoardSurface;
use super::{EntityRef, Point, RawEvent, Session, Tool, BOARD_H, BOARD_W};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// Portrait, like the board it represents.
pub const BOARD_PX_W: u32 = 384;
pub const BOARD_PX_H: u32 = 496;

/// Grass, markings and ink, in the stadium's palette.
const TURF: [u8; 3] = [99, 166, 38];
const TURF_DARK: [u8; 3] = [86, 150, 32];
const CHALK: [u8; 3] = [250, 240, 189];
const MARK_INK: [u8; 3] = [13, 10, 15];

/// A token on the board, and where it stands.
#[derive(Component)]
pub struct Token {
    pub entity: EntityRef,
    pub at: Point,
}

/// The board's face in the world, kept as vectors so picking is plain
/// arithmetic rather than transform archaeology every frame.
#[derive(Resource, Clone, Copy)]
pub struct BoardPlane {
    pub centre: Vec3,
    /// Unit vector across the face, the +u direction.
    pub right: Vec3,
    /// Unit vector up the face. Texture rows run the other way.
    pub up: Vec3,
}

impl BoardPlane {
    pub fn normal(&self) -> Vec3 {
        self.right.cross(self.up).normalize()
    }

    /// A board position as a point in the world.
    pub fn world(&self, at: Point) -> Vec3 {
        self.centre + self.right * ((at.x - 0.5) * BOARD_W) + self.up * ((0.5 - at.y) * BOARD_H)
    }

    /// Where a ray meets the face, or `None` if it misses.
    pub fn hit(&self, origin: Vec3, dir: Vec3) -> Option<Point> {
        let normal = self.normal();
        let denom = dir.dot(normal);
        if denom.abs() < 1e-5 {
            return None;
        }
        let t = (self.centre - origin).dot(normal) / denom;
        if t <= 0.0 {
            return None;
        }
        let local = origin + dir * t - self.centre;
        let at = Point::new(
            local.dot(self.right) / BOARD_W + 0.5,
            0.5 - local.dot(self.up) / BOARD_H,
        );
        ((0.0..=1.0).contains(&at.x) && (0.0..=1.0).contains(&at.y)).then_some(at)
    }
}

/// Marks drawn on the board, as normalised point lists with their log ids.
/// An arrow gets a head; a freehand line does not.
#[derive(Resource, Default)]
pub struct Marks {
    pub committed: Vec<(u32, Vec<Point>, bool)>,
    pub drawing: Vec<Point>,
    pub drawing_is_arrow: bool,
}

/// The token currently under the hand.
#[derive(Resource)]
pub struct Dragging {
    entity: Entity,
    which: EntityRef,
    from: Point,
    started_at_ms: u64,
}

/// A fresh board: turf, mowing bands and the markings, attacking up and down.
pub fn blank_board_image() -> Image {
    let (w, h) = (BOARD_PX_W as usize, BOARD_PX_H as usize);
    let mut data = vec![255u8; w * h * 4];
    let put = |data: &mut Vec<u8>, x: usize, y: usize, rgb: [u8; 3]| {
        if x < w && y < h {
            let i = (y * w + x) * 4;
            data[i..i + 3].copy_from_slice(&rgb);
        }
    };
    // Mowing bands run across the pitch, so they read as depth up the board.
    for y in 0..h {
        for x in 0..w {
            let band = (y * 10 / h) % 2 == 0;
            put(&mut data, x, y, if band { TURF } else { TURF_DARK });
        }
    }
    let line = ((w as f32) * 0.008).max(2.0) as usize;
    let rect = |data: &mut Vec<u8>, x0: f32, y0: f32, x1: f32, y1: f32| {
        let (px0, py0) = ((x0 * w as f32) as usize, (y0 * h as f32) as usize);
        let (px1, py1) = ((x1 * w as f32) as usize, (y1 * h as f32) as usize);
        for y in py0..py1.min(h) {
            for t in 0..line {
                put(data, px0 + t, y, CHALK);
                if px1 > t {
                    put(data, px1 - t - 1, y, CHALK);
                }
            }
        }
        for x in px0..px1.min(w) {
            for t in 0..line {
                put(data, x, py0 + t, CHALK);
                if py1 > t {
                    put(data, x, py1 - t - 1, CHALK);
                }
            }
        }
    };
    // Touchlines, both penalty areas, and the halfway line across the middle.
    rect(&mut data, 0.04, 0.03, 0.96, 0.97);
    rect(&mut data, 0.22, 0.03, 0.78, 0.15);
    rect(&mut data, 0.22, 0.85, 0.78, 0.97);
    for x in (0.04 * w as f32) as usize..(0.96 * w as f32) as usize {
        for t in 0..line {
            put(&mut data, x, h / 2 + t, CHALK);
        }
    }
    // Centre circle.
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    let radius = w as f32 * 0.19;
    for step in 0..1440 {
        let a = step as f32 / 1440.0 * std::f32::consts::TAU;
        for t in 0..line {
            let r = radius + t as f32;
            put(&mut data, (cx + a.cos() * r) as usize, (cy + a.sin() * r) as usize, CHALK);
        }
    }
    Image::new(
        Extent3d { width: BOARD_PX_W, height: BOARD_PX_H, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    )
}

fn dab(data: &mut [u8], at: Point, radius_px: f32) {
    let (w, h) = (BOARD_PX_W as i32, BOARD_PX_H as i32);
    let cx = at.x * w as f32;
    let cy = at.y * h as f32;
    let r = radius_px.max(1.0);
    for y in ((cy - r) as i32).max(0)..=((cy + r) as i32).min(h - 1) {
        for x in ((cx - r) as i32).max(0)..=((cx + r) as i32).min(w - 1) {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            if dx * dx + dy * dy > r * r {
                continue;
            }
            let i = ((y * w + x) as usize) * 4;
            data[i..i + 3].copy_from_slice(&MARK_INK);
        }
    }
}

/// Paints one mark: a line along its points, and a head if it is an arrow.
pub fn paint_mark(data: &mut [u8], points: &[Point], arrow: bool) {
    for pair in points.windows(2) {
        let span = (pair[1] - pair[0]).length();
        let steps = (span * BOARD_PX_H as f32 / 1.5).ceil().clamp(1.0, 160.0) as u32;
        for step in 0..=steps {
            dab(data, pair[0].lerp(pair[1], step as f32 / steps as f32), 2.4);
        }
    }
    if !arrow {
        return;
    }
    let Some((last, from)) = points.last().zip(points.iter().rev().nth(4).or(points.first())) else {
        return;
    };
    let dir = (*last - *from).normalize_or_zero();
    if dir == Vec2::ZERO {
        return;
    }
    for side in [-1.0_f32, 1.0] {
        let angle = side * 2.5;
        let barb = Vec2::new(
            dir.x * angle.cos() - dir.y * angle.sin(),
            dir.x * angle.sin() + dir.y * angle.cos(),
        );
        for step in 0..=16 {
            dab(data, *last + barb * (step as f32 / 16.0) * 0.05, 2.4);
        }
    }
}

/// Repaints the board from the committed marks. Used after an undo or a reset,
/// where replaying is the only way back to exactly what was there before.
pub fn repaint(image: &mut Image, marks: &Marks) {
    image.data = blank_board_image().data;
    for (_, points, arrow) in &marks.committed {
        paint_mark(&mut image.data, points, *arrow);
    }
}

fn board_ray(
    windows: &Query<&Window>,
    cameras: &Query<(&Camera, &GlobalTransform), With<crate::creation::camera::CreationCamera>>,
) -> Option<(Vec3, Vec3)> {
    let cursor = windows.get_single().ok()?.cursor_position()?;
    let (camera, transform) = cameras.get_single().ok()?;
    let ray = camera.viewport_to_world(transform, cursor)?;
    Some((ray.origin, *ray.direction))
}

/// Picks a token up, moves it, and logs where it went.
#[allow(clippy::too_many_arguments)]
pub fn drag(
    mut c: Commands,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<crate::creation::camera::CreationCamera>>,
    plane: Option<Res<BoardPlane>>,
    mut tokens: Query<(Entity, &mut Token, &mut Transform)>,
    buttons: Res<ButtonInput<MouseButton>>,
    dragging: Option<ResMut<Dragging>>,
    mut session: ResMut<Session>,
    tool: Res<Tool>,
) {
    let Some(plane) = plane else {
        return;
    };
    if *tool != Tool::Move {
        return;
    }

    if buttons.just_released(MouseButton::Left) {
        if let Some(held) = dragging {
            let to = tokens.get(held.entity).map(|(_, t, _)| t.at).unwrap_or(held.from);
            if (to - held.from).length() > 0.004 {
                let at_ms = session.elapsed_ms;
                session.append(RawEvent::EntityMoved {
                    entity: held.which,
                    from: held.from,
                    to,
                    started_at_ms: held.started_at_ms,
                    at_ms,
                });
            }
            c.remove_resource::<Dragging>();
        }
        return;
    }
    if !buttons.pressed(MouseButton::Left) {
        return;
    }
    let Some((origin, dir)) = board_ray(&windows, &cameras) else {
        return;
    };
    let Some(at) = plane.hit(origin, dir) else {
        return;
    };

    if buttons.just_pressed(MouseButton::Left) && dragging.is_none() {
        let hit_world = plane.world(at);
        let mut best: Option<(f32, Entity, EntityRef, Point)> = None;
        for (entity, token, _) in &tokens {
            let distance = (plane.world(token.at) - hit_world).length();
            if distance < 0.30 && best.map_or(true, |(b, ..)| distance < b) {
                best = Some((distance, entity, token.entity, token.at));
            }
        }
        if let Some((_, entity, which, from)) = best {
            c.insert_resource(Dragging {
                entity,
                which,
                from,
                // Recorded so the payload can say when the movement began, not
                // just when the hand let go.
                started_at_ms: session.elapsed_ms,
            });
        }
        return;
    }

    if let Some(held) = dragging {
        if let Ok((_, mut token, mut transform)) = tokens.get_mut(held.entity) {
            token.at = at;
            transform.translation = plane.world(at) + plane.normal() * super::TOKEN_PROUD;
        }
    }
}

/// Draws on the board: an arrow with the arrow tool, a plain line with the pen.
#[allow(clippy::too_many_arguments)]
pub fn draw_mark(
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<crate::creation::camera::CreationCamera>>,
    plane: Option<Res<BoardPlane>>,
    surfaces: Query<&Handle<StandardMaterial>, With<BoardSurface>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut marks: ResMut<Marks>,
    mut session: ResMut<Session>,
    buttons: Res<ButtonInput<MouseButton>>,
    tool: Res<Tool>,
) {
    let Some(plane) = plane else {
        return;
    };
    let Ok(handle) = surfaces.get_single() else {
        return;
    };
    let Some(texture) = mats.get(handle).and_then(|m| m.base_color_texture.clone()) else {
        return;
    };

    // An undo or a reset removes marks from the log; the picture has to follow.
    let removed: Vec<u32> = session
        .events
        .iter()
        .filter_map(|e| match e {
            RawEvent::AnnotationRemoved { id, .. } => Some(*id),
            _ => None,
        })
        .collect();
    if !removed.is_empty() {
        let before = marks.committed.len();
        marks.committed.retain(|(id, ..)| !removed.contains(id));
        if marks.committed.len() != before {
            if let Some(image) = images.get_mut(&texture) {
                repaint(image, &marks);
            }
            let _ = mats.get_mut(handle);
        }
    }

    let arrow = match *tool {
        Tool::Arrow => true,
        Tool::Pen => false,
        _ => return,
    };

    if buttons.just_released(MouseButton::Left) && !marks.drawing.is_empty() {
        let points = std::mem::take(&mut marks.drawing);
        let was_arrow = marks.drawing_is_arrow;
        if points.len() > 1 {
            let id = session.next_id();
            let at_ms = session.elapsed_ms;
            session.append(RawEvent::AnnotationAdded {
                id,
                points: points.clone(),
                arrow: was_arrow,
                at_ms,
            });
            marks.committed.push((id, points, was_arrow));
            // The head is only drawn once the stroke is finished, because only
            // then is its direction known.
            if let Some(image) = images.get_mut(&texture) {
                repaint(image, &marks);
            }
        } else if let Some(image) = images.get_mut(&texture) {
            // A stray click left a dot; take it back off.
            repaint(image, &marks);
        }
        let _ = mats.get_mut(handle);
        return;
    }
    if !buttons.pressed(MouseButton::Left) {
        return;
    }
    let Some((origin, dir)) = board_ray(&windows, &cameras) else {
        return;
    };
    let Some(at) = plane.hit(origin, dir) else {
        return;
    };
    if marks.drawing.is_empty() {
        marks.drawing_is_arrow = arrow;
    }
    // Repainting the image is not enough on its own: the material holds the
    // texture binding it was built with, so it has to be touched for the
    // renderer to pick the marked-up board up. Same lesson as the canvas.
    let _ = mats.get_mut(handle);
    let last = marks.drawing.last().copied();
    if last.map_or(true, |p| (at - p).length() > 0.004) {
        marks.drawing.push(at);
        if let (Some(previous), Some(image)) = (last, images.get_mut(&texture)) {
            paint_mark(&mut image.data, &[previous, at], false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An upright board, leaned back a little, as the table carries it.
    fn upright() -> BoardPlane {
        let lean = Quat::from_rotation_x(-0.22);
        BoardPlane {
            centre: Vec3::new(10., 3., -5.),
            right: lean * Vec3::X,
            up: lean * Vec3::Y,
        }
    }

    #[test]
    fn a_ray_at_the_middle_of_the_board_reads_as_its_centre() {
        let plane = upright();
        let at = plane
            .hit(plane.centre + plane.normal() * 4., -plane.normal())
            .expect("straight at the face hits it");
        assert!((at - Point::new(0.5, 0.5)).length() < 1e-5);
    }

    #[test]
    fn board_positions_and_world_positions_round_trip() {
        let plane = upright();
        for at in [Point::new(0.1, 0.9), Point::new(0.5, 0.5), Point::new(0.83, 0.2)] {
            let world = plane.world(at);
            let back = plane
                .hit(world + plane.normal() * 3., -plane.normal())
                .expect("on the board");
            assert!((back - at).length() < 1e-5, "{at:?} came back as {back:?}");
        }
    }

    #[test]
    fn the_top_of_the_board_is_higher_in_the_world_than_the_bottom() {
        // Texture rows run down while the board stands up; getting this
        // backwards silently mirrors every tactic.
        let plane = upright();
        let top = plane.world(Point::new(0.5, 0.0));
        let bottom = plane.world(Point::new(0.5, 1.0));
        assert!(top.y > bottom.y, "top {top:?} bottom {bottom:?}");
    }

    #[test]
    fn rays_that_miss_the_board_report_nothing() {
        let plane = upright();
        let off = plane.centre + plane.right * BOARD_W + plane.normal() * 4.;
        assert!(plane.hit(off, -plane.normal()).is_none());
        // Pointing away from the face.
        assert!(plane.hit(plane.centre + plane.normal() * 4., plane.normal()).is_none());
    }

    #[test]
    fn a_turned_board_still_picks_correctly() {
        // The island is rotated, so the board's axes are not the world's.
        let turn = Quat::from_rotation_y(2.3) * Quat::from_rotation_x(-0.22);
        let plane =
            BoardPlane { centre: Vec3::new(-4., 2., 7.), right: turn * Vec3::X, up: turn * Vec3::Y };
        let at = Point::new(0.2, 0.75);
        let back = plane.hit(plane.world(at) + plane.normal() * 2., -plane.normal()).unwrap();
        assert!((back - at).length() < 1e-5);
    }

    #[test]
    fn the_blank_board_is_portrait_turf_with_markings_on_it() {
        assert!(BOARD_PX_H > BOARD_PX_W, "the board stands taller than it is wide");
        let image = blank_board_image();
        assert_eq!(image.data.len(), (BOARD_PX_W * BOARD_PX_H * 4) as usize);
        assert!(image.data.chunks(4).all(|p| p[3] == 255));
        let chalk = image.data.chunks(4).filter(|p| p[0] == CHALK[0] && p[1] == CHALK[1]).count();
        assert!(chalk > 500, "the markings are drawn ({chalk} pixels)");
    }

    #[test]
    fn the_board_texture_matches_the_face_it_is_stretched_over() {
        let texture = BOARD_PX_W as f32 / BOARD_PX_H as f32;
        let face = BOARD_W / BOARD_H;
        assert!((texture - face).abs() < 0.03, "{texture} vs {face}: markings would stretch");
    }

    #[test]
    fn a_mark_inks_the_board_along_its_length() {
        let mut image = blank_board_image();
        paint_mark(&mut image.data, &[Point::new(0.5, 0.3), Point::new(0.5, 0.7)], true);
        let middle = ((BOARD_PX_H / 2 * BOARD_PX_W + BOARD_PX_W / 2) * 4) as usize;
        assert_eq!(&image.data[middle..middle + 3], &MARK_INK, "ink along the shaft");
        assert_ne!(&image.data[0..3], &MARK_INK, "and not across the whole board");
    }
}
