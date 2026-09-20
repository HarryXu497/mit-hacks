//! The plinth, the upright board, the tokens and the sign's timber.
//!
//! Built from the stadium's own [`Kit`], in the clearing's frame, so the table
//! belongs to the island the easel already stands on.

use super::board::{blank_board_image, BoardPlane, Marks, Token};
use super::{
    EntityRef, BOARD_H, BOARD_LEAN, BOARD_W, SIGN_FOOT, SIGN_H, SIGN_LOCAL, SIGN_W, SIGN_YAW,
    TABLE_LOCAL, TABLE_TOP, TOKEN_PROUD,
};
use crate::creation::{facing, place};
use crate::jungle::{beam, material, Kit, INK};
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

/// The board's painted face, so the marking pass can find it.
#[derive(Component)]
pub struct BoardSurface;

/// The sign's face, whose texture the sign camera renders into.
#[derive(Component)]
pub struct SignFace;

/// Starting arrangement, portrait: a keeper, two lines of four, and the ball on
/// the spot. The ids are the fixed roster the tactical contract names — 1 to 5
/// defending the top, 6 to 10 the bottom — shown in the match's own orange and
/// blue rather than in the contract's colour words, because this board depicts
/// the match about to be played.
const START: [(EntityRef, Vec2); 11] = [
    (EntityRef::Player(1), Vec2::new(0.50, 0.08)),
    (EntityRef::Player(2), Vec2::new(0.26, 0.22)),
    (EntityRef::Player(3), Vec2::new(0.74, 0.22)),
    (EntityRef::Player(4), Vec2::new(0.36, 0.36)),
    (EntityRef::Player(5), Vec2::new(0.64, 0.36)),
    (EntityRef::Player(6), Vec2::new(0.36, 0.64)),
    (EntityRef::Player(7), Vec2::new(0.64, 0.64)),
    (EntityRef::Player(8), Vec2::new(0.26, 0.78)),
    (EntityRef::Player(9), Vec2::new(0.74, 0.78)),
    (EntityRef::Player(10), Vec2::new(0.50, 0.92)),
    (EntityRef::Ball, Vec2::new(0.50, 0.50)),
];

/// Where a token belongs when the board is reset.
pub fn start_position(entity: EntityRef) -> Option<Vec2> {
    START.iter().find(|(e, _)| *e == entity).map(|(_, at)| *at)
}

/// Where the board's centre sits above the plinth.
fn board_centre_local() -> Vec3 {
    TABLE_LOCAL + Vec3::Y * (TABLE_TOP + 0.22 + BOARD_H * 0.5)
}

/// The board's own turn: the clearing's heading, then leaned back like a board
/// propped against its stand.
pub fn board_lean() -> Quat {
    facing() * Quat::from_rotation_x(-BOARD_LEAN)
}

pub fn build_table(
    mut c: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    sign_visible: Res<super::SignVisible>,
) {
    let k = crate::jungle::build_kit(&mut meshes, &mut mats);
    let quad = meshes.add(Rectangle::new(1., 1.));

    plinth(&mut c, &k);

    let lean = board_lean();
    let centre = place(board_centre_local());
    let plane = BoardPlane { centre, right: lean * Vec3::X, up: lean * Vec3::Y };

    let face = images.add(blank_board_image());
    let surface = mats.add(StandardMaterial {
        base_color_texture: Some(face),
        // Unlit for the same reason the canvas is: the markings and the drawn
        // lines are art, not lit surfaces, and banding would lose thin lines.
        unlit: true,
        ..default()
    });
    // Ink shell and timber frame behind the face.
    for (mat, size, offset) in [
        (k.ink.clone(), Vec3::new(BOARD_W + INK * 8., BOARD_H + INK * 8., 0.12), -0.16),
        (k.wood_light.clone(), Vec3::new(BOARD_W + 0.32, BOARD_H + 0.32, 0.12), -0.08),
    ] {
        c.spawn(PbrBundle {
            mesh: k.cube.clone(),
            material: mat,
            transform: Transform::from_translation(centre + lean * Vec3::new(0., 0., offset))
                .with_rotation(lean)
                .with_scale(size),
            ..default()
        });
    }
    c.spawn((
        PbrBundle {
            mesh: quad.clone(),
            material: surface,
            // On the plane itself, which is what the picking maths assumes and
            // what the tokens stand proud of.
            transform: Transform::from_translation(centre)
                .with_rotation(lean)
                .with_scale(Vec3::new(BOARD_W, BOARD_H, 1.)),
            ..default()
        },
        BoardSurface,
        crate::rendering::batching::NoMerge,
    ));
    // Two struts from the plinth to the back of the board.
    for side in [-1.0_f32, 1.0] {
        beam(
            &mut c,
            &k,
            k.wood.clone(),
            place(TABLE_LOCAL + Vec3::new(side * BOARD_W * 0.36, TABLE_TOP, 0.55)),
            centre + lean * Vec3::new(side * BOARD_W * 0.42, -BOARD_H * 0.34, -0.12),
            0.14,
        );
    }

    c.insert_resource(plane);
    c.init_resource::<Marks>();

    let digits = digit_textures(&mut images);
    for (entity, at) in START {
        token(&mut c, &k, &mut mats, &quad, &digits, plane, entity, at);
    }

    if sign_visible.0 {
        sign_timber(&mut c, &k, &mut mats, &quad);
    }
}

/// The stone plinth the board stands on.
fn plinth(c: &mut Commands, k: &Kit) {
    for tier in 0..3 {
        let t = tier as f32;
        c.spawn(PbrBundle {
            mesh: k.cube.clone(),
            material: if tier == 1 { k.rock_light.clone() } else { k.rock.clone() },
            transform: Transform::from_translation(place(TABLE_LOCAL + Vec3::Y * (0.18 + t * 0.34)))
                .with_rotation(facing())
                .with_scale(Vec3::new(BOARD_W + 1.1 - t * 0.26, 0.36, 1.9 - t * 0.2)),
            ..default()
        });
    }
    // A timber shelf across the top, and rope along its front edge.
    c.spawn(PbrBundle {
        mesh: k.cube.clone(),
        material: k.wood.clone(),
        transform: Transform::from_translation(place(TABLE_LOCAL + Vec3::Y * (TABLE_TOP - 0.1)))
            .with_rotation(facing())
            .with_scale(Vec3::new(BOARD_W + 1.4, 0.28, 2.1)),
        ..default()
    });
    beam(
        c,
        k,
        k.rope.clone(),
        place(TABLE_LOCAL + Vec3::new(-BOARD_W * 0.6, TABLE_TOP - 0.1, 1.02)),
        place(TABLE_LOCAL + Vec3::new(BOARD_W * 0.6, TABLE_TOP - 0.1, 1.02)),
        0.07,
    );
}

/// One token: a carved disc in its team's colour, with its number on its face.
///
/// The disc is a child of an unscaled parent, so the number sitting in front of
/// it is not stretched by the disc's own flattening — which is what hid the
/// numbers when the disc itself was the parent.
#[allow(clippy::too_many_arguments)]
fn token(
    c: &mut Commands,
    k: &Kit,
    mats: &mut Assets<StandardMaterial>,
    quad: &Handle<Mesh>,
    digits: &[Handle<Image>; 11],
    plane: BoardPlane,
    entity: EntityRef,
    at: Vec2,
) {
    let (colour, radius) = match entity {
        EntityRef::Player(id) if id <= 5 => (k.orange.clone(), 0.17),
        EntityRef::Player(_) => (k.blue.clone(), 0.17),
        EntityRef::Ball => (k.white.clone(), 0.11),
    };
    let lean = board_lean();
    let root = c
        .spawn((
            SpatialBundle::from_transform(
                Transform::from_translation(plane.world(at) + plane.normal() * TOKEN_PROUD)
                    .with_rotation(lean),
            ),
            Token { entity, at },
            crate::rendering::batching::NoMerge,
        ))
        .id();

    // The disc, lying against the board's face, and its ink shell.
    c.spawn(PbrBundle {
        mesh: k.stone.clone(),
        material: colour,
        transform: Transform::from_scale(Vec3::new(radius, radius, 0.07)),
        ..default()
    })
    .set_parent(root);
    c.spawn(PbrBundle {
        mesh: k.stone.clone(),
        material: k.ink.clone(),
        transform: Transform::from_scale(Vec3::new(radius + INK, radius + INK, 0.07 + INK)),
        ..default()
    })
    .set_parent(root);

    // The number, on the face of the disc.
    if let EntityRef::Player(number) = entity {
        let face = mats.add(StandardMaterial {
            base_color_texture: Some(digits[number as usize].clone()),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
        c.spawn(PbrBundle {
            mesh: quad.clone(),
            material: face,
            transform: Transform::from_translation(Vec3::new(0., 0., 0.09))
                .with_scale(Vec3::splat(radius * 1.8)),
            ..default()
        })
        .set_parent(root);
    }
}

/// The sign's posts and frame. Its face is filled in by `sign.rs`.
fn sign_timber(c: &mut Commands, k: &Kit, mats: &mut Assets<StandardMaterial>, quad: &Handle<Mesh>) {
    let (w, h) = (SIGN_W, SIGN_H);
    let turn = facing() * Quat::from_rotation_y(SIGN_YAW);
    let centre = place(SIGN_LOCAL + Vec3::Y * (SIGN_FOOT + h * 0.5));
    for side in [-1.0_f32, 1.0] {
        beam(
            c,
            k,
            k.wood_light.clone(),
            place(SIGN_LOCAL + turn * Vec3::new(side * w * 0.42, 0., 0.)),
            place(SIGN_LOCAL + turn * Vec3::new(side * w * 0.42, SIGN_FOOT + h * 0.6, 0.)),
            0.19,
        );
    }
    for (mat, size, z) in [
        (k.ink.clone(), Vec3::new(w + INK * 8., h + INK * 8., 0.12), -0.06),
        (k.wood.clone(), Vec3::new(w + 0.3, h + 0.3, 0.12), 0.0),
    ] {
        c.spawn(PbrBundle {
            mesh: k.cube.clone(),
            material: mat,
            transform: Transform::from_translation(centre + turn * Vec3::new(0., 0., z))
                .with_rotation(turn)
                .with_scale(size),
            ..default()
        });
    }
    // The face. `sign.rs` swaps in the rendered texture once it exists.
    let blank = material(mats, Color::rgb(0.05, 0.06, 0.07));
    c.spawn((
        PbrBundle {
            mesh: quad.clone(),
            material: blank,
            transform: Transform::from_translation(centre + turn * Vec3::new(0., 0., 0.08))
                .with_rotation(turn)
                .with_scale(Vec3::new(w, h, 1.)),
            ..default()
        },
        SignFace,
        crate::rendering::batching::NoMerge,
    ));
}

/// A 3x5 dot-matrix digit, which is all a shirt number needs to be readable.
const GLYPHS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b001, 0b001, 0b001],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

/// Ivory numbers 0..=10 on a transparent ground, one texture each.
fn digit_textures(images: &mut Assets<Image>) -> [Handle<Image>; 11] {
    std::array::from_fn(|n| images.add(number_image(n)))
}

const TILE: usize = 32;

fn number_image(n: usize) -> Image {
    let digits: Vec<usize> = if n >= 10 { vec![n / 10, n % 10] } else { vec![n] };
    // Dot size chosen so the whole number fits the tile: a two-digit number
    // needs seven dot-widths across, a one-digit number only three, and both
    // need five down. Sized rather than assumed, because 10 overflowed.
    let across = 4 * digits.len() - 1;
    let cell = ((TILE - 2) / across).min((TILE - 2) / 5).max(1);
    let glyph_w = 3 * cell;
    let gap = cell;
    let total_w = digits.len() * glyph_w + (digits.len() - 1) * gap;
    let total_h = 5 * cell;
    let ox = (TILE - total_w) / 2;
    let oy = (TILE - total_h) / 2;

    let mut data = vec![0u8; TILE * TILE * 4];
    for (index, digit) in digits.iter().enumerate() {
        for (row, bits) in GLYPHS[*digit].iter().enumerate() {
            for col in 0..3 {
                if bits & (0b100 >> col) == 0 {
                    continue;
                }
                for dy in 0..cell {
                    for dx in 0..cell {
                        let x = ox + index * (glyph_w + gap) + col * cell + dx;
                        let y = oy + row * cell + dy;
                        let i = (y * TILE + x) * 4;
                        // Ivory, so a number reads on both team colours.
                        data[i] = 250;
                        data[i + 1] = 240;
                        data[i + 2] = 189;
                        data[i + 3] = 255;
                    }
                }
            }
        }
    }
    Image::new(
        Extent3d { width: TILE as u32, height: TILE as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD | RenderAssetUsages::MAIN_WORLD,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_board_starts_five_a_side_with_the_ball_on_the_spot() {
        assert_eq!(START.len(), 11, "ten players and a ball, fixed");
        let ball = START.iter().find(|(e, _)| *e == EntityRef::Ball).unwrap().1;
        assert_eq!(ball, Vec2::new(0.5, 0.5));
        let home = START.iter().filter(|(e, _)| matches!(e, EntityRef::Player(id) if *id <= 5));
        let away = START.iter().filter(|(e, _)| matches!(e, EntityRef::Player(id) if *id > 5));
        assert_eq!(home.count(), 5);
        assert_eq!(away.count(), 5);
    }

    #[test]
    fn every_token_starts_on_the_board() {
        for (entity, at) in START {
            assert!(
                (0.0..=1.0).contains(&at.x) && (0.0..=1.0).contains(&at.y),
                "{entity:?} starts off the board at {at:?}"
            );
        }
    }

    #[test]
    fn the_two_sides_start_in_their_own_halves_up_and_down_the_board() {
        // Portrait: the halves are top and bottom, not left and right.
        for (entity, at) in START {
            match entity {
                EntityRef::Player(id) if id <= 5 => assert!(at.y < 0.5, "{id} is not at home"),
                EntityRef::Player(id) => assert!(at.y > 0.5, "{id} is not at home"),
                EntityRef::Ball => {}
            }
        }
    }

    #[test]
    fn no_two_tokens_start_on_top_of_each_other() {
        for (i, (a, pa)) in START.iter().enumerate() {
            for (b, pb) in START.iter().skip(i + 1) {
                assert!((*pa - *pb).length() > 0.08, "{a:?} and {b:?} overlap");
            }
        }
    }

    #[test]
    fn every_shirt_number_fits_its_tile_with_ink_on_a_clear_ground() {
        // 1 through 10: the two-digit one is the one that overflowed.
        for n in 1..=10 {
            let image = number_image(n);
            assert_eq!(image.data.len(), TILE * TILE * 4, "{n}");
            let inked = image.data.chunks(4).filter(|p| p[3] == 255).count();
            assert!(inked > 0, "{n} is drawn");
            assert!(inked < TILE * TILE / 2, "{n} leaves most of the tile clear");
        }
    }

    #[test]
    fn the_board_stands_up_off_its_plinth() {
        let centre = board_centre_local();
        assert!(centre.y > TABLE_TOP + BOARD_H * 0.4, "the face clears the shelf");
    }
}
