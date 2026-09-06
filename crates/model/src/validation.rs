//! Shared validation for native commands, project loading, history and automation.
use crate::{
    Easing, FrameRate, LayerKind, Project, PropValue, Property, Time, Track, TICKS_PER_SEC,
};
use std::collections::BTreeSet;

pub fn validate_comp_size(
    width: u32,
    height: u32,
    fps: FrameRate,
    duration: Time,
) -> Result<(), String> {
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || u64::from(width) * u64::from(height) > 16_777_216
    {
        return Err(
            "Composition dimensions must be 1–8192 pixels, with at most 16 megapixels".into(),
        );
    }
    if fps.num == 0 || fps.den == 0 || fps.as_f64() < 1.0 || fps.as_f64() > 240.0 {
        return Err("Frame rate must be between 1 and 240 fps".into());
    }
    if (TICKS_PER_SEC as u64 * fps.den as u64) % fps.num as u64 != 0 {
        return Err("This frame rate cannot be represented exactly in 120,000 ticks/second".into());
    }
    if duration.0 <= 0 || duration.0 > 86_400 * TICKS_PER_SEC {
        return Err("Composition duration must be positive and no longer than 24 hours".into());
    }
    Ok(())
}

fn finite(v: f32) -> bool {
    v.is_finite() && v.abs() <= 1_000_000.0
}
fn color(v: &[f32; 4]) -> bool {
    v.iter().all(|x| x.is_finite() && (0.0..=1.0).contains(x))
}

pub fn validate_track(track: &Track, property: Option<Property>) -> Result<(), String> {
    if track.keys.len() > 10_000 {
        return Err("A track supports at most 10,000 keys".into());
    }
    if !track.keys.windows(2).all(|k| k[0].time < k[1].time) {
        return Err("Keyframes must have unique, ascending times".into());
    }
    let first_kind = track
        .keys
        .first()
        .map(|k| matches!(k.value, PropValue::Vec2(_)));
    for key in &track.keys {
        let is_vec = matches!(key.value, PropValue::Vec2(_));
        let valid_type = match property {
            Some(Property::Position | Property::Scale | Property::AnchorPoint) => is_vec,
            Some(_) => !is_vec,
            None => Some(is_vec) == first_kind,
        };
        let valid_value = match key.value {
            PropValue::Scalar(v) => finite(v),
            PropValue::Vec2(v) => v.into_iter().all(finite),
        };
        if !valid_type
            || !valid_value
            || key.time.0.unsigned_abs() > (86_400 * TICKS_PER_SEC) as u64
        {
            return Err("Invalid keyframe value, type or time".into());
        }
        if let Easing::Bezier { p1, p2 } = key.easing {
            if !p1.into_iter().chain(p2).all(finite)
                || !(0.0..=1.0).contains(&p1[0])
                || !(0.0..=1.0).contains(&p2[0])
            {
                return Err(
                    "Bézier time handles must be between 0 and 1; values must be finite".into(),
                );
            }
        }
    }
    Ok(())
}

pub fn validate_content(kind: &LayerKind) -> Result<(), String> {
    match kind {
        LayerKind::Solid { color: c } if !color(c) => return Err("Invalid fill color".into()),
        LayerKind::Shape {
            color: c, style, ..
        } => {
            if !color(c)
                || !color(&style.stroke_color)
                || !finite(style.corner_radius)
                || style.corner_radius < 0.0
                || !finite(style.stroke_width)
                || style.stroke_width < 0.0
            {
                return Err("Invalid shape style".into());
            }
            if let Some(size) = style.size {
                if !size
                    .into_iter()
                    .all(|v| v.is_finite() && v > 0.0 && v <= 8192.0)
                    || size[0].ceil() * size[1].ceil() > 16_777_216.0
                {
                    return Err("Shape dimensions must be positive, at most 8192 pixels, and fit within 16 megapixels".into());
                }
            }
        }
        LayerKind::Text { text, size, style } => {
            if text.len() > 16_384
                || !size.is_finite()
                || *size < 1.0
                || *size > 2048.0
                || !color(&style.color)
                || !finite(style.tracking)
                || style.tracking < -(*size * 0.5)
            {
                return Err("Invalid text size, content, tracking or color".into());
            }
        }
        _ => {}
    }
    Ok(())
}

impl Project {
    pub fn validate(&self) -> Result<(), String> {
        // JSON numbers cross a JavaScript client. Keep counters representable
        // there and far below u64 overflow before any allocator can be called.
        const MAX_SAFE_ID: u64 = 9_007_199_254_740_991;
        if [self.next_comp.0, self.next_layer.0, self.next_media.0]
            .into_iter()
            .any(|n| n == 0 || n > MAX_SAFE_ID)
        {
            return Err("Document ID allocators must be positive, safe JSON integers".into());
        }
        if self.name.len() > 1024 || self.comps.len() > 128 || self.media.len() > 1024 {
            return Err("Project exceeds document limits".into());
        }
        for (id, comp) in &self.comps {
            if *id != comp.id || id.0 == 0 || id.0 >= self.next_comp.0 {
                return Err("Invalid composition ID allocator".into());
            }
            validate_comp_size(comp.width, comp.height, comp.fps, comp.duration)?;
            if !color(&comp.background) || comp.layers.len() > 2048 {
                return Err("Invalid composition background or layer count".into());
            }
            let order: BTreeSet<_> = comp.layer_order.iter().copied().collect();
            if order.len() != comp.layer_order.len()
                || order != comp.layers.keys().copied().collect()
            {
                return Err("Layer order must contain every layer exactly once".into());
            }
            for (layer_id, layer) in &comp.layers {
                if *layer_id != layer.id || layer_id.0 == 0 || layer_id.0 >= self.next_layer.0 {
                    return Err("Invalid layer ID allocator".into());
                }
                if layer.duration.0 < 0
                    || layer.duration.0 > 86_400 * TICKS_PER_SEC
                    || layer.start.0.unsigned_abs() > (86_400 * TICKS_PER_SEC) as u64
                {
                    return Err("Invalid layer timing".into());
                }
                let t = &layer.transform;
                if !t
                    .position
                    .into_iter()
                    .chain(t.scale)
                    .chain(t.anchor_point)
                    .chain([t.rotation, t.opacity])
                    .all(finite)
                    || !(0.0..=1.0).contains(&t.opacity)
                {
                    return Err(
                        "Transform values must be finite; opacity must be between 0 and 1".into(),
                    );
                }
                validate_content(&layer.kind)?;
                crate::effect::validate_effect_stack(&layer.effects)?;
                for (property, track) in &layer.tracks {
                    validate_track(track, Some(*property))?;
                }
                let mut chain = BTreeSet::new();
                chain.insert(layer.id);
                let mut parent = layer.parent;
                while let Some(parent_id) = parent {
                    if !chain.insert(parent_id) {
                        return Err("Layer parenting contains a cycle".into());
                    }
                    parent = comp
                        .layers
                        .get(&parent_id)
                        .ok_or("Parent layer is missing; unparent children before deleting")?
                        .parent;
                }
                match layer.kind {
                    LayerKind::PreComp { comp: child } if !self.comps.contains_key(&child) => {
                        return Err("Referenced composition is missing".into())
                    }
                    LayerKind::Footage { media } if !self.media.contains_key(&media) => {
                        return Err("Referenced media is missing".into())
                    }
                    _ => {}
                }
            }
            self.validate_precomp(*id, &mut Vec::new())?;
        }
        let mut image_bytes = 0usize;
        for (id, asset) in &self.media {
            if *id != asset.id || id.0 == 0 || id.0 >= self.next_media.0 {
                return Err("Invalid media ID allocator".into());
            }
            if let Some(image) = &asset.embedded {
                if image.width == 0
                    || image.height == 0
                    || image.width > 4096
                    || image.height > 4096
                    || u64::from(image.width) * u64::from(image.height) > 4_194_304
                {
                    return Err("Embedded images are limited to 4 megapixels".into());
                }
                let expected = (image.width as usize * image.height as usize * 4).div_ceil(3) * 4;
                if image.rgba_base64.len() != expected {
                    return Err("Embedded image byte length does not match its dimensions".into());
                }
                image_bytes = image_bytes.saturating_add(expected);
            }
        }
        if image_bytes > 48 * 1024 * 1024 {
            return Err("Embedded images exceed the 48 MB project budget".into());
        }
        Ok(())
    }

    fn validate_precomp(
        &self,
        id: crate::CompId,
        active: &mut Vec<crate::CompId>,
    ) -> Result<(), String> {
        if active.contains(&id) || active.len() >= 32 {
            return Err("Nested composition cycle or depth limit (32) exceeded".into());
        }
        active.push(id);
        for layer in self
            .comps
            .get(&id)
            .ok_or("Composition not found")?
            .layers
            .values()
        {
            if let LayerKind::PreComp { comp } = layer.kind {
                self.validate_precomp(comp, active)?;
            }
        }
        active.pop();
        Ok(())
    }
}
