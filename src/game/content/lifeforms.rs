//! Built-in lifeforms are authored here as one stable-ID definition table.

use super::{LifeformDefinition, LifeformId};

impl LifeformId {
    pub const WALKER: Self = Self::new(1);
    pub const GLOWGNAT: Self = Self::new(2);
}

/// Used directly by spawning, movement, combat, and visual selection.
pub const BUILT_IN_LIFEFORMS: &[LifeformDefinition] = &[
    LifeformDefinition::walker(LifeformId::WALKER, "Surface Walker"),
    LifeformDefinition::glowgnat(LifeformId::GLOWGNAT, "Glowgnat"),
];
