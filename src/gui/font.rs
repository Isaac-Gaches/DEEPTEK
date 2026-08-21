use crate::SpriteAtlasFrame;
use easy_gpu::assets::{Texture, TextureBuilder};
use easy_gpu::assets_manager::Handle;
use easy_gpu::wgpu::{Extent3d, TextureFormat};
use fontdue::{Font, FontSettings, Metrics};
use std::collections::HashMap;
use std::sync::OnceLock;

const FONT_BYTES: &[u8] = include_bytes!("../../assets/fonts/Orbitron-Medium.ttf");
const ATLAS_SIZE: u32 = 512;
const ATLAS_PADDING: u32 = 1;
const FIRST_CHARACTER: u8 = b' ';
const LAST_CHARACTER: u8 = b'~';

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum TextSize {
    Small,
    Medium,
    Large,
}

impl TextSize {
    const ALL: [Self; 3] = [Self::Small, Self::Medium, Self::Large];

    pub(super) const fn pixels(self) -> f32 {
        match self {
            Self::Small => 8.0,
            Self::Medium => 12.0,
            Self::Large => 16.0,
        }
    }

    pub(super) fn from_legacy_scale(scale: f32) -> Self {
        if scale < 1.25 {
            Self::Small
        } else if scale < 1.75 {
            Self::Medium
        } else {
            Self::Large
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Glyph {
    pub frame: u32,
    pub metrics: Metrics,
}

pub(super) struct FontAtlas {
    pub texture: Handle<Texture>,
    pub frames: Vec<SpriteAtlasFrame>,
    glyphs: HashMap<(TextSize, char), Glyph>,
}

impl FontAtlas {
    pub(super) fn new(gpu: &mut easy_gpu::Renderer) -> Self {
        let mut pixels = vec![0_u8; (ATLAS_SIZE * ATLAS_SIZE * 4) as usize];
        let mut frames = Vec::new();
        let mut glyphs = HashMap::new();
        let mut shelf_x = ATLAS_PADDING;
        let mut shelf_y = ATLAS_PADDING;
        let mut shelf_height = 0_u32;

        for size in TextSize::ALL {
            for byte in FIRST_CHARACTER..=LAST_CHARACTER {
                let character = char::from(byte);
                let (metrics, coverage) = font().rasterize(character, size.pixels());
                if metrics.width == 0 || metrics.height == 0 {
                    glyphs.insert(
                        (size, character),
                        Glyph {
                            frame: u32::MAX,
                            metrics,
                        },
                    );
                    continue;
                }

                let glyph_width = metrics.width as u32;
                let glyph_height = metrics.height as u32;
                if shelf_x + glyph_width + ATLAS_PADDING > ATLAS_SIZE {
                    shelf_x = ATLAS_PADDING;
                    shelf_y += shelf_height + ATLAS_PADDING;
                    shelf_height = 0;
                }
                assert!(
                    shelf_y + glyph_height + ATLAS_PADDING <= ATLAS_SIZE,
                    "Orbitron glyph atlas is too small"
                );

                for y in 0..glyph_height {
                    for x in 0..glyph_width {
                        let alpha = coverage[(y * glyph_width + x) as usize];
                        let destination = (((shelf_y + y) * ATLAS_SIZE + shelf_x + x) * 4) as usize;
                        pixels[destination..destination + 4]
                            .copy_from_slice(&[255, 255, 255, alpha]);
                    }
                }

                let frame = frames.len() as u32;
                frames.push(SpriteAtlasFrame::new(
                    [
                        shelf_x as f32 / ATLAS_SIZE as f32,
                        shelf_y as f32 / ATLAS_SIZE as f32,
                    ],
                    [
                        (shelf_x + glyph_width) as f32 / ATLAS_SIZE as f32,
                        (shelf_y + glyph_height) as f32 / ATLAS_SIZE as f32,
                    ],
                ));
                glyphs.insert((size, character), Glyph { frame, metrics });
                shelf_x += glyph_width + ATLAS_PADDING;
                shelf_height = shelf_height.max(glyph_height);
            }
        }

        let extent = Extent3d {
            width: ATLAS_SIZE,
            height: ATLAS_SIZE,
            depth_or_array_layers: 1,
        };
        let texture = TextureBuilder::new()
            .size(extent)
            .format(TextureFormat::Rgba8UnormSrgb)
            .build(gpu);
        gpu.write_texture(texture, &pixels, 4, extent);

        Self {
            texture,
            frames,
            glyphs,
        }
    }

    pub(super) fn glyph(&self, size: TextSize, character: char) -> Option<Glyph> {
        self.glyphs
            .get(&(size, character))
            .or_else(|| self.glyphs.get(&(size, '?')))
            .copied()
    }
}

pub(super) fn font() -> &'static Font {
    static FONT: OnceLock<Font> = OnceLock::new();
    FONT.get_or_init(|| {
        Font::from_bytes(FONT_BYTES, FontSettings::default())
            .expect("embedded Orbitron font is valid")
    })
}

pub(super) fn line_metrics(size: TextSize) -> fontdue::LineMetrics {
    font()
        .horizontal_line_metrics(size.pixels())
        .expect("Orbitron has horizontal line metrics")
}

pub(super) fn text_width(text: &str, size: TextSize) -> f32 {
    let mut width = 0.0_f32;
    let mut previous = None;
    for character in text.chars().take_while(|&character| character != '\n') {
        if let Some(previous) = previous {
            width += font()
                .horizontal_kern(previous, character, size.pixels())
                .unwrap_or(0.0);
        }
        width += font().metrics(character, size.pixels()).advance_width;
        previous = Some(character);
    }
    width
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_orbitron_has_ui_glyphs_and_metrics() {
        assert!(font().has_glyph('A'));
        assert!(font().has_glyph('a'));
        assert!(font().has_glyph('?'));
        assert!(text_width("WIDE", TextSize::Large) > text_width("III", TextSize::Large));
    }
}
