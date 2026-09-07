use std::ops::Range;

use gpui::{BackdropGlass, DrawOrder, PrimitiveBatch, Scene};

pub(super) fn instance_range(range: Range<usize>) -> Range<u32> {
    range.start as u32..range.end as u32
}

pub(super) fn backdrop_glass_render_pass_count(pass_count: u32) -> usize {
    pass_count as usize * 2 + 2 + usize::from(pass_count > 0)
}

pub(super) fn planned_backdrop_glass_pass_count(
    blur: &BackdropGlass,
    remaining_gaussian_render_passes: &mut usize,
) -> u32 {
    let requested = blur.gaussian_pass_count().unwrap_or(0);
    let requested_passes = requested as usize * 2;
    if requested_passes <= *remaining_gaussian_render_passes {
        *remaining_gaussian_render_passes -= requested_passes;
        requested
    } else {
        0
    }
}

pub(super) fn batch_first_order(scene: &Scene, batch: &PrimitiveBatch) -> DrawOrder {
    match batch {
        PrimitiveBatch::Shadows(range) => scene.shadows[range.start].order,
        PrimitiveBatch::Quads(range) => scene.quads[range.start].order,
        PrimitiveBatch::Paths(range) => scene.paths[range.start].order,
        PrimitiveBatch::Underlines(range) => scene.underlines[range.start].order,
        PrimitiveBatch::MonochromeSprites { range, .. } => {
            scene.monochrome_sprites[range.start].order
        }
        PrimitiveBatch::SubpixelSprites { range, .. } => scene.subpixel_sprites[range.start].order,
        PrimitiveBatch::PolychromeSprites { range, .. } => {
            scene.polychrome_sprites[range.start].order
        }
        PrimitiveBatch::Surfaces(range) => scene.surfaces[range.start].order,
        PrimitiveBatch::ExternalImages { range, .. } => {
            scene.external_images[range.start].sprite.order
        }
    }
}
