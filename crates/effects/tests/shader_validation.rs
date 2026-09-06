#[test]
fn bundled_wgsl_parses_and_validates_without_claiming_gpu_execution() {
    let registry = bonaparte_effects::EffectRegistry::new();
    for (id, effect) in registry.iter() {
        let module = naga::front::wgsl::parse_str(&effect.shader_source)
            .unwrap_or_else(|e| panic!("{id}: {}", e.emit_to_string(&effect.shader_source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap_or_else(|e| panic!("{id}: {e:?}"));
    }
}
