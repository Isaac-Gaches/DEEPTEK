use easy_gpu::assets::Material;
use easy_gpu::assets_manager::Handle;

/// World-space position, rotation, and non-uniform scale for an entity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub position: [f32; 2],
    pub rotation: f32,
    pub scale: [f32; 2],
}

impl Transform {
    pub const fn new(position: [f32; 2]) -> Self {
        Self {
            position,
            rotation: 0.0,
            scale: [1.0, 1.0],
        }
    }

    pub const fn with_scale(mut self, scale: [f32; 2]) -> Self {
        self.scale = scale;
        self
    }

    pub const fn with_rotation(mut self, rotation: f32) -> Self {
        self.rotation = rotation;
        self
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::new([0.0, 0.0])
    }
}

/// An instanced sprite referencing a renderer-owned material and atlas frame.
#[derive(Clone, Copy)]
pub struct Sprite {
    pub material: Handle<Material>,
    pub frame: u32,
    pub tint: [f32; 4],
    /// Blends from terrain lighting at zero to fully self-lit at one.
    pub emissive: f32,
    /// WebGPU depth in the `0.0..=1.0` range. Lower values are nearer.
    pub depth: f32,
}

impl Sprite {
    pub const fn new(material: Handle<Material>) -> Self {
        Self {
            material,
            frame: 0,
            tint: [1.0; 4],
            emissive: 0.0,
            depth: 0.1,
        }
    }

    pub const fn with_frame(mut self, frame: u32) -> Self {
        self.frame = frame;
        self
    }

    pub const fn with_tint(mut self, tint: [f32; 4]) -> Self {
        self.tint = tint;
        self
    }

    pub fn with_emissive(mut self, emissive: f32) -> Self {
        self.emissive = emissive.clamp(0.0, 1.0);
        self
    }

    pub fn with_depth(mut self, depth: f32) -> Self {
        self.depth = depth.clamp(0.0, 1.0);
        self
    }
}

/// An axis-aligned tile collider with lightweight rigid-body state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Collider {
    pub half_extents: [f32; 2],
    pub offset: [f32; 2],
    pub velocity: [f32; 2],
    pub restitution: f32,
    pub friction: f32,
    pub gravity_scale: f32,
    pub angular_velocity: f32,
    pub angular_drag: f32,
    pub rotation_radius: f32,
    pub rotation_enabled: bool,
    pub linear_drag: f32,
    pub ground_drag: f32,
    pub on_ground: bool,
    /// Set for the frame after horizontal terrain contact.
    pub hit_wall: bool,
    pub enabled: bool,
}

impl Collider {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            half_extents: [(width * 0.5).max(0.001), (height * 0.5).max(0.001)],
            offset: [0.0, 0.0],
            velocity: [0.0, 0.0],
            restitution: 0.0,
            friction: 0.0,
            gravity_scale: 1.0,
            angular_velocity: 0.0,
            angular_drag: 0.0,
            rotation_radius: 0.5,
            rotation_enabled: false,
            linear_drag: 0.0,
            ground_drag: 0.0,
            on_ground: false,
            hit_wall: false,
            enabled: true,
        }
    }

    pub const fn with_offset(mut self, offset: [f32; 2]) -> Self {
        self.offset = offset;
        self
    }

    pub const fn with_velocity(mut self, velocity: [f32; 2]) -> Self {
        self.velocity = velocity;
        self
    }

    pub fn with_material(mut self, restitution: f32, friction: f32) -> Self {
        self.restitution = restitution.clamp(0.0, 1.0);
        self.friction = friction.max(0.0);
        self
    }

    pub const fn with_gravity_scale(mut self, gravity_scale: f32) -> Self {
        self.gravity_scale = gravity_scale;
        self
    }

    pub fn with_angular_motion(
        mut self,
        angular_velocity: f32,
        angular_drag: f32,
        rotation_radius: f32,
    ) -> Self {
        self.angular_velocity = angular_velocity;
        self.angular_drag = angular_drag.max(0.0);
        self.rotation_radius = rotation_radius.max(0.01);
        self.rotation_enabled = true;
        self
    }

    /// Adds continuous air resistance and extra horizontal drag while grounded.
    pub fn with_drag(mut self, linear_drag: f32, ground_drag: f32) -> Self {
        self.linear_drag = linear_drag.max(0.0);
        self.ground_drag = ground_drag.max(0.0);
        self
    }
}

pub(super) fn move_towards(current: f32, target: f32, maximum_delta: f32) -> f32 {
    if (target - current).abs() <= maximum_delta {
        target
    } else {
        current + (target - current).signum() * maximum_delta
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Health {
    current: u16,
    maximum: u16,
}

impl Health {
    pub fn new(maximum: u16) -> Self {
        Self {
            current: maximum,
            maximum,
        }
    }

    pub const fn current(self) -> u16 {
        self.current
    }

    pub const fn maximum(self) -> u16 {
        self.maximum
    }

    pub fn heal(&mut self, amount: u16) -> u16 {
        let healed = amount.min(self.maximum - self.current);
        self.current += healed;
        healed
    }

    pub fn damage(&mut self, amount: u16) -> u16 {
        let damage = amount.min(self.current);
        self.current -= damage;
        damage
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Energy {
    current: u16,
    maximum: u16,
}

impl Energy {
    pub fn new(maximum: u16) -> Self {
        Self {
            current: maximum,
            maximum,
        }
    }

    pub const fn current(self) -> u16 {
        self.current
    }

    pub const fn maximum(self) -> u16 {
        self.maximum
    }

    pub fn recharge(&mut self, amount: u16) -> u16 {
        let restored = amount.min(self.maximum - self.current);
        self.current += restored;
        restored
    }

    pub fn spend(&mut self, amount: u16) -> bool {
        if amount > self.current {
            return false;
        }
        self.current -= amount;
        true
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Wallet {
    money: u64,
}

impl Wallet {
    pub const fn new(money: u64) -> Self {
        Self { money }
    }

    pub const fn money(self) -> u64 {
        self.money
    }

    pub fn deposit(&mut self, amount: u64) {
        self.money = self.money.saturating_add(amount);
    }

    pub fn withdraw(&mut self, amount: u64) -> bool {
        let Some(remaining) = self.money.checked_sub(amount) else {
            return false;
        };
        self.money = remaining;
        true
    }
}

#[cfg(test)]
mod resource_tests {
    use super::*;

    #[test]
    fn energy_never_overfills_or_spends_more_than_available() {
        let mut energy = Energy::new(100);
        assert!(energy.spend(35));
        assert!(!energy.spend(66));
        assert_eq!(energy.current(), 65);
        assert_eq!(energy.recharge(80), 35);
        assert_eq!(energy.current(), 100);
    }

    #[test]
    fn wallet_changes_are_checked_and_deposits_saturate() {
        let mut wallet = Wallet::new(25);
        assert!(!wallet.withdraw(26));
        assert!(wallet.withdraw(10));
        wallet.deposit(u64::MAX);
        assert_eq!(wallet.money(), u64::MAX);
    }
}
