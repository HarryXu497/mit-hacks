#[test]
fn fonts_parse_with_the_engine_text_backend() {
    for name in ["MPLUSRounded1c-Black", "MPLUSRounded1c-Bold"] {
        let path = format!("assets/fonts/{name}.ttf");
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let font = bevy::text::Font::try_from_bytes(bytes)
            .unwrap_or_else(|e| panic!("{path} rejected: {e:?}"));
        use ab_glyph::{Font as _, ScaleFont as _};
        let scaled = font.font.as_scaled(64.0);
        for ch in ['C', 'a', '3', '·', 'é', '—'] {
            let id = font.font.glyph_id(ch);
            assert_ne!(id.0, 0, "{name} is missing {ch:?}");
            assert!(scaled.h_advance(id) > 0.0, "{name}: {ch:?} has no width");
        }
    }
}
