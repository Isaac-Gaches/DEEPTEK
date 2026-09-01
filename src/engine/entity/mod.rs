mod camera;
mod components;
mod effects;
mod hazards;
mod lifeform;
mod physics;
mod player;
mod projectile;
mod turret;

pub use camera::FollowCamera;
pub use components::{Collider, Energy, Health, Sprite, Transform, Wallet};
pub use effects::{Bomb, EffectsMaterials, EffectsSystem, Particle, ParticleKind, spawn_bomb};
pub use hazards::{
    SPIKE_CONTACT_DAMAGE, SPIKE_DAMAGE_INTERVAL_SECONDS, SpikeDamageSystem, SpikeDamageUpdate,
};
pub use lifeform::{
    BUILT_IN_LIFEFORMS, GLOWGNAT_MIN_MACHINERY_ATTENTION, Lifeform, LifeformDefinition, LifeformId,
    LifeformLocomotion, LifeformMaterials, LifeformSimulation, LifeformSimulationConfig,
    LifeformSimulationUpdate, LifeformSpawnView, LifeformSystem, LifeformSystemError,
    LifeformVisual, built_in_lifeform_definitions,
};
pub use physics::{PhysicsConfig, update_colliders};
pub use player::{
    Player, PlayerInput, entity_position, spawn_player, update_player_animation, update_players,
};
pub use projectile::{DynamicLight, Lifetime, Projectile, ProjectileSystem, spawn_glowstick};
pub use turret::{MAX_TURRET_ELEVATION_DEGREES, TurretProjectile, TurretStats, TurretSystem};
