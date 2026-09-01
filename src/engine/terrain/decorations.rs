use super::ObjectTypeId;

/// Stable IDs for naturally placed decorations.
pub struct NaturalObject;

impl NaturalObject {
    pub const GRASS: ObjectTypeId = ObjectTypeId::new(1);
    pub const PEBBLE: ObjectTypeId = ObjectTypeId::new(2);
    pub const VINE: ObjectTypeId = ObjectTypeId::new(3);
    pub const HANGING_STONE: ObjectTypeId = ObjectTypeId::new(6);
}

/// Stable ID for the player-placeable rope decoration.
pub const ROPE_OBJECT: ObjectTypeId = ObjectTypeId::new(4);
pub const POWERED_CABLE_OBJECT: ObjectTypeId = ObjectTypeId::new(5);

/// Atlas behaviour understood by the instanced decoration renderer. Variant
/// decorations select one persistent frame when placed; segmented growth is a
/// separate reusable form for vines and similar hanging objects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecorationVisual {
    Variants { first_frame: u16, variants: u8 },
    Static { frame: u16 },
    Segmented { body_frame: u16, tip_frame: u16 },
    Rope,
    PoweredCable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecorationDefinition {
    object_type: ObjectTypeId,
    name: &'static str,
    visual: DecorationVisual,
    first_update_delay: Option<u64>,
}

impl DecorationDefinition {
    pub const fn new(
        object_type: ObjectTypeId,
        name: &'static str,
        visual: DecorationVisual,
        first_update_delay: Option<u64>,
    ) -> Self {
        Self {
            object_type,
            name,
            visual,
            first_update_delay,
        }
    }

    pub const fn object_type(self) -> ObjectTypeId {
        self.object_type
    }

    pub const fn name(self) -> &'static str {
        self.name
    }

    pub const fn visual(self) -> DecorationVisual {
        self.visual
    }

    pub const fn first_update_delay(self) -> Option<u64> {
        self.first_update_delay
    }
}

pub const BUILT_IN_DECORATIONS: &[DecorationDefinition] = &[
    DecorationDefinition::new(
        NaturalObject::GRASS,
        "Grass",
        DecorationVisual::Variants {
            first_frame: 3,
            variants: 2,
        },
        None,
    ),
    DecorationDefinition::new(
        NaturalObject::PEBBLE,
        "Pebble",
        DecorationVisual::Static { frame: 5 },
        None,
    ),
    DecorationDefinition::new(
        NaturalObject::VINE,
        "Vine",
        DecorationVisual::Segmented {
            body_frame: 1,
            tip_frame: 2,
        },
        Some(8),
    ),
    DecorationDefinition::new(
        NaturalObject::HANGING_STONE,
        "Hanging Stone",
        DecorationVisual::Static { frame: 0 },
        None,
    ),
    DecorationDefinition::new(ROPE_OBJECT, "Rope", DecorationVisual::Rope, None),
    DecorationDefinition::new(
        POWERED_CABLE_OBJECT,
        "Powered Cable",
        DecorationVisual::PoweredCable,
        None,
    ),
];

pub fn decoration_definition(object_type: ObjectTypeId) -> Option<DecorationDefinition> {
    BUILT_IN_DECORATIONS
        .iter()
        .copied()
        .find(|definition| definition.object_type == object_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_decoration_ids_are_unique() {
        for (index, definition) in BUILT_IN_DECORATIONS.iter().enumerate() {
            assert!(
                BUILT_IN_DECORATIONS[index + 1..]
                    .iter()
                    .all(|other| other.object_type != definition.object_type)
            );
        }
    }
}
