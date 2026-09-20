#import bevy_pbr::{forward_io::{VertexOutput, FragmentOutput}, pbr_fragment::pbr_input_from_standard_material, pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing}}
#import bevy_pbr::mesh_view_bindings::globals

@group(2) @binding(100) var<uniform> settings: vec4<f32>;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) front: bool) -> FragmentOutput {
    var p = pbr_input_from_standard_material(in, front);
    var out: FragmentOutput;
    let pos = p.world_position.xyz;
    if settings.x > 0.5 {
        let t = globals.time;
        let horizontal = abs(p.N.y) > 0.5;
        let u = pos.x * 1.8 + pos.z * 0.55;
        let v = select(pos.y * 3.0 + t * 5.0, pos.z * 2.4 - t * 1.7, horizontal);
        let wave = sin(u + t * 0.9) * cos(v) + sin(u * 2.3 - v * 0.7) * 0.35;
        if horizontal {
            p.N = normalize(p.N + vec3<f32>(cos(u + t) * 0.13, 0., sin(v) * 0.16));
        }
        let fresnel = pow(1. - max(dot(p.N, p.V), 0.), 4.);
        let deep = vec3<f32>(0.018, 0.19, 0.23);
        let shallow = vec3<f32>(0.07, 0.50, 0.43);
        var foam = smoothstep(1.05, 1.28, wave) * 0.33;
#ifdef VERTEX_UVS
        if horizontal {
            let edge = abs(in.uv.x - 0.5) * 2.;
            foam += smoothstep(0.83, 1., edge + wave * 0.055) * 0.7;
        } else {
            foam += pow(0.5 + 0.5 * sin(u * 6. + sin(v)), 12.) * 0.45;
        }
#endif
        p.material.base_color = vec4<f32>(mix(deep, shallow, 0.5 + wave * 0.13), 1.);
        p.material.perceptual_roughness = 0.19;
        out.color = apply_pbr_lighting(p);
        out.color = vec4<f32>(mix(out.color.rgb, vec3<f32>(0.43, 0.69, 0.76), fresnel * 0.6) + vec3<f32>(foam), 1.);
    } else {
        // Small patches break the perfectly flat mowing stripes without noisy textures.
        if pos.y > 1.09 && pos.y < 1.15 && abs(pos.x) < 24. && abs(pos.z) < 16. {
            let grass = sin(pos.x * 7.3 + sin(pos.z * 6.)) * sin(pos.z * 9.1)
                + sin(pos.x * 1.6 + pos.z * 2.3) * 0.5;
            p.material.base_color = vec4<f32>(p.material.base_color.rgb * (1. + grass * 0.035), p.material.base_color.a);
        }
        let lit = apply_pbr_lighting(p);
        let base = max(p.material.base_color.rgb, vec3<f32>(0.025));
        let illumination = dot(lit.rgb / base, vec3<f32>(0.2126, 0.7152, 0.0722));
        // Quantize illumination, not RGB: keeps the expanded palette intact.
        // Fewer, harder steps than a soft ramp: the target look is poster-flat.
        // The darkest band is lifted off zero. With ambient this low, a face
        // turned away from the sun quantised to pure black and lost its colour
        // entirely; shade should be a darker version of the form, not a hole.
        let bands = max(floor(illumination * 3.0 + 0.5) / 3.0, 0.36);
        let stepped = max(mix(illumination, bands, 0.88), 0.30);
        var shaded = lit.rgb * stepped / max(illumination, 0.001);
        // settings.y marks an actor surface: players, the ball, anything that has
        // to read as a figure against the field. Actors carry their own near-black
        // contour and a near-white hotspot, which is what separates subject from
        // ground in this style -- the ground itself never contains either.
        if settings.y > 0.5 {
            let facing = max(dot(p.N, p.V), 0.0);
            // Tight exponent: on a part only ~20px across, a soft rim swallows the
            // whole silhouette and the kit colour stops reading. This keeps the
            // contour to the outer edge and leaves the face of each form lit.
            // Outline geometry carries the contour again, so the rim only has to
            // keep a form's edge from going flat rather than draw the edge itself.
            let rim = pow(1.0 - facing, 4.0);
            shaded = shaded * (1.0 - rim * 0.34);
            // The hotspot is a highlight, so it only belongs where light lands.
            // Unscaled, a flat face seen head-on in shade was bleached to grey.
            let hot = pow(facing, 7.0) * 0.30 * clamp(illumination * 1.4 - 0.3, 0.0, 1.0);
            shaded = shaded + vec3<f32>(hot);
        }
        out.color = vec4<f32>(shaded, lit.a);
    }
    return FragmentOutput(main_pass_post_lighting_processing(p, out.color));
}
