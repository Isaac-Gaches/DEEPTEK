use super::{CHUNK_SIZE, ChunkPos, SEA_LEVEL_PERCENT, TilePos};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct BiomeId(u8);

impl BiomeId {
    pub const NORMAL: Self = Self(0);
    pub const GLOWING_CRYSTAL: Self = Self(1);

    pub const fn new(value: u8) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u8 {
        self.0
    }

    pub const fn is_known(self) -> bool {
        matches!(self, Self::NORMAL | Self::GLOWING_CRYSTAL)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::NORMAL => "Normal",
            Self::GLOWING_CRYSTAL => "Glowing Crystal",
            _ => "Unknown",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BiomeMap {
    chunks_wide: u32,
    chunks_high: u32,
    cells: Box<[BiomeId]>,
}

impl BiomeMap {
    pub(crate) fn normal(chunks_wide: u32, chunks_high: u32) -> Self {
        let count = chunks_wide as usize * chunks_high as usize;
        Self {
            chunks_wide,
            chunks_high,
            cells: vec![BiomeId::NORMAL; count].into_boxed_slice(),
        }
    }

    pub(crate) fn generate(seed: u64, width: u32, height: u32) -> Self {
        let chunks_wide = width.div_ceil(CHUNK_SIZE as u32);
        let chunks_high = height.div_ceil(CHUNK_SIZE as u32);
        let mut map = Self::normal(chunks_wide, chunks_high);

        // Keep the crystal region below the chunk containing sea level. Tiny
        // test worlds without a wholly underground chunk remain normal.
        let surface_chunk = (height * SEA_LEVEL_PERCENT / 100) / CHUNK_SIZE as u32;
        let first_underground = surface_chunk.saturating_add(1);
        if chunks_wide == 0 || first_underground >= chunks_high {
            return map;
        }

        let x_hash = mix(seed ^ 0xB10B_1E00_4352_5953);
        let y_hash = mix(seed ^ 0xC2A5_7A11_554E_4445);
        let centre = ChunkPos {
            x: (x_hash % u64::from(chunks_wide)) as u32,
            y: first_underground + (y_hash % u64::from(chunks_high - first_underground)) as u32,
        };
        let radius_x = chunks_wide.div_ceil(10).clamp(1, 6);
        let underground_height = chunks_high - first_underground;
        let radius_y = underground_height.div_ceil(8).clamp(1, 5);

        let minimum_x = centre.x.saturating_sub(radius_x);
        let maximum_x = centre.x.saturating_add(radius_x).min(chunks_wide - 1);
        let minimum_y = centre.y.saturating_sub(radius_y).max(first_underground);
        let maximum_y = centre.y.saturating_add(radius_y).min(chunks_high - 1);
        let radius_x_sq = i64::from(radius_x) * i64::from(radius_x);
        let radius_y_sq = i64::from(radius_y) * i64::from(radius_y);
        let limit = radius_x_sq * radius_y_sq;
        for y in minimum_y..=maximum_y {
            for x in minimum_x..=maximum_x {
                let dx = i64::from(x) - i64::from(centre.x);
                let dy = i64::from(y) - i64::from(centre.y);
                if dx * dx * radius_y_sq + dy * dy * radius_x_sq <= limit {
                    map.set(ChunkPos { x, y }, BiomeId::GLOWING_CRYSTAL);
                }
            }
        }
        map
    }

    pub const fn chunks_wide(&self) -> u32 {
        self.chunks_wide
    }

    pub const fn chunks_high(&self) -> u32 {
        self.chunks_high
    }

    pub fn get(&self, chunk: ChunkPos) -> Option<BiomeId> {
        self.index(chunk).map(|index| self.cells[index])
    }

    pub fn cells(&self) -> &[BiomeId] {
        &self.cells
    }

    pub(crate) fn set(&mut self, chunk: ChunkPos, biome: BiomeId) -> bool {
        let Some(index) = self.index(chunk) else {
            return false;
        };
        self.cells[index] = biome;
        true
    }

    pub(crate) fn replace_cells(&mut self, cells: Box<[BiomeId]>) -> bool {
        if cells.len() != self.cells.len() {
            return false;
        }
        self.cells = cells;
        true
    }

    fn index(&self, chunk: ChunkPos) -> Option<usize> {
        (chunk.x < self.chunks_wide && chunk.y < self.chunks_high)
            .then(|| (chunk.y * self.chunks_wide + chunk.x) as usize)
    }
}

impl super::World {
    pub fn biome_map(&self) -> &BiomeMap {
        &self.biomes
    }

    pub fn biome_in_chunk(&self, chunk: ChunkPos) -> Option<BiomeId> {
        self.biomes.get(chunk)
    }

    pub fn biome_at(&self, position: TilePos) -> Option<BiomeId> {
        if position.x >= self.width || position.y >= self.height {
            return None;
        }
        self.biome_in_chunk(position.chunk())
    }

    pub fn set_biome_in_chunk(&mut self, chunk: ChunkPos, biome: BiomeId) -> bool {
        biome.is_known() && self.biomes.set(chunk, biome)
    }

    pub(crate) fn generate_biomes(&mut self) {
        self.biomes = BiomeMap::generate(self.seed, self.width, self.height);
    }
}

fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9E37_79B9_7F4A_7C15);
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_crystal_region_is_deterministic_and_underground() {
        let first = BiomeMap::generate(73, 1_024, 1_024);
        let second = BiomeMap::generate(73, 1_024, 1_024);
        assert_eq!(first, second);
        let first_underground = (1_024 * SEA_LEVEL_PERCENT / 100) / CHUNK_SIZE as u32 + 1;
        let crystals: Vec<_> = first
            .cells()
            .iter()
            .enumerate()
            .filter_map(|(index, &biome)| {
                (biome == BiomeId::GLOWING_CRYSTAL).then_some(ChunkPos {
                    x: index as u32 % first.chunks_wide(),
                    y: index as u32 / first.chunks_wide(),
                })
            })
            .collect();
        assert!(!crystals.is_empty());
        assert!(crystals.iter().all(|chunk| chunk.y >= first_underground));
    }

    #[test]
    fn different_seeds_move_the_crystal_region() {
        assert_ne!(
            BiomeMap::generate(1, 1_024, 1_024),
            BiomeMap::generate(2, 1_024, 1_024)
        );
    }
}
