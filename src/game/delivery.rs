use crate::{
    Collider, ContractCompany, CorporationProgress, Inventory, ItemId, ItemRegistry, Sprite,
    Transform, Wallet,
};
use easy_gpu::assets::Material;
use easy_gpu::assets_manager::Handle;
use hecs::{Entity, World as EntityWorld};
use std::collections::VecDeque;
use std::sync::LazyLock;

pub const DELIVERY_DELAY_SECONDS: f32 = 15.0;
pub const PROGRAM_DELIVERY_DELAY_SECONDS: f32 = DELIVERY_DELAY_SECONDS;
const MAX_PENDING_DELIVERIES: usize = 1_024;
const DELIVERY_CRATE_PICKUP_RANGE: [f32; 2] = [2.25, 2.5];
pub const DELIVERY_DROP_HEIGHT: f32 = 40.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MachineOffer {
    pub item: ItemId,
    pub company: ContractCompany,
    pub price: u64,
    pub minimum_company_level: u8,
    pub description: &'static str,
}

impl MachineOffer {
    pub fn is_unlocked(self, progress: CorporationProgress) -> bool {
        progress.level(self.company) >= self.minimum_company_level
    }

    pub fn can_purchase(self, progress: CorporationProgress, money: u64) -> bool {
        self.is_unlocked(progress) && money >= self.price
    }
}

/// Shop catalogue derived from the same furniture definitions used by item
/// placement and simulation. Adding a definition with `with_item` and
/// `with_offer` automatically makes it purchasable.
pub static MACHINE_OFFERS: LazyLock<Vec<MachineOffer>> = LazyLock::new(|| {
    let mut offers: Vec<_> = crate::BUILT_IN_FURNITURE
        .iter()
        .filter_map(|definition| {
            let item = definition.item()?;
            let offer = definition.offer()?;
            Some(MachineOffer {
                item: item.id,
                company: offer.company,
                price: offer.price,
                minimum_company_level: offer.minimum_company_level,
                description: offer.description,
            })
        })
        .collect();
    // Preserve the established catalogue presentation while allowing newly
    // registered offers to append automatically.
    offers.sort_by_key(|offer| match offer.item {
        ItemId::LASER_BORE => 0,
        ItemId::RED_SHAFT_BORE => 1,
        ItemId::COMPOSITE_ASSEMBLER => 2,
        ItemId::TURRET => 3,
        ItemId::AMMO_TURRET => 4,
        ItemId::DIRECTIONAL_SENTRY => 5,
        ItemId::ORBITAL_EXPORT_LAUNCHER => 6,
        ItemId::SOLAR_ARRAY => 7,
        ItemId::BATTERY => 8,
        ItemId::CARGO_LIFT => 9,
        ItemId::LIFT_STATION => 10,
        ItemId::LASER_DRILL => 11,
        ItemId::SUBSURFACE_SURVEYOR => 12,
        ItemId::DOOR => 13,
        ItemId::BED => 14,
        _ => usize::MAX,
    });
    offers
});

pub fn machine_offer(item: ItemId) -> Option<&'static MachineOffer> {
    MACHINE_OFFERS.iter().find(|offer| offer.item == item)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PendingDelivery {
    item: ItemId,
    arrives_at: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PurchaseError {
    ItemNotOffered,
    InsufficientFunds,
    CorporationLevelRequired,
    QueueFull,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleDeliveryError {
    InvalidDelay,
    QueueFull,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeliveryCrate {
    item: ItemId,
}

impl DeliveryCrate {
    pub const fn item(self) -> ItemId {
        self.item
    }
}

/// Owns the monotonic procurement clock and its arrival-ordered delivery queue.
/// Updating an idle queue is O(1); a frame only performs work for crates that become due.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeliverySystem {
    elapsed_seconds: f32,
    pending: VecDeque<PendingDelivery>,
    drop_sequence: u32,
}

// Scheduling rejects non-finite values, so equality remains reflexive.
impl Eq for DeliverySystem {}

impl DeliverySystem {
    pub fn purchase(
        &mut self,
        item: ItemId,
        wallet: &mut Wallet,
        progress: CorporationProgress,
    ) -> Result<(), PurchaseError> {
        let offer = machine_offer(item).ok_or(PurchaseError::ItemNotOffered)?;
        if self.pending.len() >= MAX_PENDING_DELIVERIES {
            return Err(PurchaseError::QueueFull);
        }
        if !offer.is_unlocked(progress) {
            return Err(PurchaseError::CorporationLevelRequired);
        }
        if !wallet.withdraw(offer.price) {
            return Err(PurchaseError::InsufficientFunds);
        }
        self.enqueue(item, DELIVERY_DELAY_SECONDS);
        Ok(())
    }

    pub fn schedule_batch(
        &mut self,
        items: &[ItemId],
        delay_seconds: f32,
    ) -> Result<(), ScheduleDeliveryError> {
        if !delay_seconds.is_finite() || delay_seconds < 0.0 {
            return Err(ScheduleDeliveryError::InvalidDelay);
        }
        if self.pending.len().saturating_add(items.len()) > MAX_PENDING_DELIVERIES {
            return Err(ScheduleDeliveryError::QueueFull);
        }
        for &item in items {
            self.enqueue(item, delay_seconds);
        }
        Ok(())
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn pending_items(&self) -> impl Iterator<Item = ItemId> + '_ {
        self.pending.iter().map(|delivery| delivery.item)
    }

    pub fn seconds_until_next(&self) -> Option<f32> {
        self.pending
            .front()
            .map(|delivery| (delivery.arrives_at - self.elapsed_seconds).max(0.0))
    }

    pub fn update_queue(
        &mut self,
        entities: &mut EntityWorld,
        crate_material: Handle<Material>,
        drop_centre_x: f32,
        drop_y: f32,
        world_width: u32,
        elapsed: f32,
    ) -> usize {
        self.elapsed_seconds += elapsed.max(0.0);
        let mut spawned = 0;
        while self
            .pending
            .front()
            .is_some_and(|delivery| delivery.arrives_at <= self.elapsed_seconds)
        {
            let delivery = self.pending.pop_front().expect("front delivery exists");
            let sequence = self.drop_sequence;
            let offset = (sequence % 5) as f32 - 2.0;
            self.drop_sequence = self.drop_sequence.wrapping_add(1);
            let maximum_x = world_width.saturating_sub(2).max(1) as f32;
            let x = (drop_centre_x + offset).clamp(1.0, maximum_x);
            spawn_delivery_crate_with_sequence(
                entities,
                crate_material,
                delivery.item,
                [x, drop_y.max(1.0)],
                sequence,
            );
            spawned += 1;
        }
        spawned
    }

    /// Collects landed crates near the player. A full inventory leaves the crate
    /// untouched so a paid delivery can never be discarded.
    pub fn collect_nearby(
        &self,
        entities: &mut EntityWorld,
        inventory: &mut Inventory,
        registry: &ItemRegistry,
        player_position: [f32; 2],
    ) -> usize {
        let mut collected = Vec::new();
        for (entity, (delivery, transform, collider)) in entities
            .query::<(&DeliveryCrate, &Transform, &Collider)>()
            .iter()
        {
            let difference = [
                (transform.position[0] - player_position[0]).abs(),
                (transform.position[1] - player_position[1]).abs(),
            ];
            if collider.on_ground
                && difference[0] <= DELIVERY_CRATE_PICKUP_RANGE[0]
                && difference[1] <= DELIVERY_CRATE_PICKUP_RANGE[1]
                && inventory.add(delivery.item, 1, registry) == 0
            {
                collected.push(entity);
            }
        }
        for &entity in &collected {
            let _ = entities.despawn(entity);
        }
        collected.len()
    }

    fn enqueue(&mut self, item: ItemId, delay_seconds: f32) {
        let delivery = PendingDelivery {
            item,
            arrives_at: self.elapsed_seconds + delay_seconds,
        };
        let index = self
            .pending
            .iter()
            .position(|pending| pending.arrives_at > delivery.arrives_at)
            .unwrap_or(self.pending.len());
        self.pending.insert(index, delivery);
    }

    pub(crate) fn saved_state(&self) -> (f32, u32, impl Iterator<Item = (ItemId, f32)> + '_) {
        (
            self.elapsed_seconds,
            self.drop_sequence,
            self.pending
                .iter()
                .map(|delivery| (delivery.item, delivery.arrives_at)),
        )
    }

    pub(crate) fn from_saved(
        elapsed_seconds: f32,
        drop_sequence: u32,
        pending: impl IntoIterator<Item = (ItemId, f32)>,
    ) -> Option<Self> {
        if !elapsed_seconds.is_finite() || elapsed_seconds < 0.0 {
            return None;
        }
        let pending: VecDeque<_> = pending
            .into_iter()
            .map(|(item, arrives_at)| {
                (arrives_at.is_finite() && arrives_at >= elapsed_seconds)
                    .then_some(PendingDelivery { item, arrives_at })
            })
            .collect::<Option<_>>()?;
        if pending.len() > MAX_PENDING_DELIVERIES
            || pending
                .iter()
                .zip(pending.iter().skip(1))
                .any(|(left, right)| left.arrives_at > right.arrives_at)
        {
            return None;
        }
        Some(Self {
            elapsed_seconds,
            pending,
            drop_sequence,
        })
    }
}

pub fn spawn_delivery_crate(
    entities: &mut EntityWorld,
    material: Handle<Material>,
    item: ItemId,
    position: [f32; 2],
) -> Entity {
    spawn_delivery_crate_with_sequence(entities, material, item, position, 0)
}

fn spawn_delivery_crate_with_sequence(
    entities: &mut EntityWorld,
    material: Handle<Material>,
    item: ItemId,
    position: [f32; 2],
    sequence: u32,
) -> Entity {
    let horizontal_velocity = signed_motion_sample(sequence, 0xA511_E9B3) * 0.65;
    let initial_rotation = signed_motion_sample(sequence, 0x63D8_35A7) * 0.24;
    let spin_sample = signed_motion_sample(sequence, 0xC2B2_AE35);
    let angular_velocity = spin_sample.signum() * (0.45 + spin_sample.abs() * 0.55);
    entities.spawn((
        DeliveryCrate { item },
        Transform::new(position)
            .with_rotation(initial_rotation)
            .with_scale([1.5, 1.5]),
        Collider::new(1.35, 1.35)
            .with_velocity([horizontal_velocity, 3.0])
            .with_material(0.12, 0.85)
            .with_angular_motion(angular_velocity, 1.2, 0.68)
            .with_drag(0.04, 8.0),
        Sprite::new(material)
            .with_frame(0)
            .with_tint([1.0, 0.72, 0.30, 1.0])
            .with_depth(0.07),
    ))
}

/// Small deterministic variations keep crate drops replayable while preventing
/// stacks from following the exact same flight path.
fn signed_motion_sample(sequence: u32, salt: u32) -> f32 {
    let mut value = sequence.wrapping_add(salt);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7FEB_352D);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846C_A68B);
    value ^= value >> 16;
    let fraction = (value >> 8) as f32 / 16_777_215.0;
    fraction * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use easy_gpu::assets_manager::Handle;
    use std::marker::PhantomData;

    fn material() -> Handle<Material> {
        Handle {
            index: 0,
            generation: 0,
            _marker: PhantomData,
        }
    }

    #[test]
    fn purchase_charges_once_and_delivers_after_fifteen_seconds() {
        let mut system = DeliverySystem::default();
        let mut wallet = Wallet::new(2_000);
        let mut entities = EntityWorld::new();

        assert_eq!(
            system.purchase(
                ItemId::LASER_BORE,
                &mut wallet,
                CorporationProgress::default()
            ),
            Ok(())
        );
        assert_eq!(wallet.money(), 500);
        assert_eq!(system.pending_count(), 1);
        assert_eq!(
            system.update_queue(&mut entities, material(), 10.0, 5.0, 30, 14.9),
            0
        );
        assert_eq!(
            system.update_queue(&mut entities, material(), 10.0, 5.0, 30, 0.1),
            1
        );
        assert_eq!(system.pending_count(), 0);
        assert_eq!(entities.query::<&DeliveryCrate>().iter().count(), 1);
    }

    #[test]
    fn rejected_purchase_never_charges_or_enters_the_queue() {
        let mut system = DeliverySystem::default();
        let mut wallet = Wallet::new(100);
        assert_eq!(
            system.purchase(
                ItemId::LASER_BORE,
                &mut wallet,
                CorporationProgress::default()
            ),
            Err(PurchaseError::InsufficientFunds)
        );
        assert_eq!(wallet.money(), 100);
        assert_eq!(system.pending_count(), 0);
    }

    #[test]
    fn program_batch_is_atomic_and_uses_the_standard_delivery_delay() {
        let mut system = DeliverySystem::default();
        let mut wallet = Wallet::new(2_000);
        system
            .purchase(
                ItemId::LASER_BORE,
                &mut wallet,
                CorporationProgress::default(),
            )
            .unwrap();
        let items = [
            ItemId::ORBITAL_EXPORT_LAUNCHER,
            ItemId::SOLAR_ARRAY,
            ItemId::PYLON,
        ];
        system
            .schedule_batch(&items, PROGRAM_DELIVERY_DELAY_SECONDS)
            .unwrap();
        assert_eq!(
            system.pending_items().collect::<Vec<_>>(),
            [
                ItemId::LASER_BORE,
                ItemId::ORBITAL_EXPORT_LAUNCHER,
                ItemId::SOLAR_ARRAY,
                ItemId::PYLON,
            ]
        );

        let mut entities = EntityWorld::new();
        assert_eq!(
            system.update_queue(
                &mut entities,
                material(),
                10.0,
                5.0,
                30,
                PROGRAM_DELIVERY_DELAY_SECONDS - 0.1,
            ),
            0
        );
        assert_eq!(
            system.update_queue(&mut entities, material(), 10.0, 5.0, 30, 0.1),
            4
        );
        assert_eq!(system.pending_count(), 0);
    }

    #[test]
    fn sequential_delivery_crates_have_bounded_varied_tumble() {
        let mut system = DeliverySystem::default();
        system
            .schedule_batch(&[ItemId::SOLAR_ARRAY, ItemId::PYLON], 0.0)
            .unwrap();
        let mut entities = EntityWorld::new();
        assert_eq!(
            system.update_queue(&mut entities, material(), 10.0, 5.0, 30, 0.0),
            2
        );

        let motion: Vec<_> = entities
            .query::<(&Transform, &Collider)>()
            .iter()
            .map(|(_, (transform, collider))| {
                (
                    transform.rotation,
                    collider.velocity[0],
                    collider.angular_velocity,
                )
            })
            .collect();
        assert_eq!(motion.len(), 2);
        assert_ne!(motion[0], motion[1]);
        for (rotation, horizontal_velocity, angular_velocity) in motion {
            assert!(rotation.abs() <= 0.24);
            assert!(horizontal_velocity.abs() <= 0.65);
            assert!((0.45..=1.0).contains(&angular_velocity.abs()));
        }
    }

    #[test]
    fn machine_catalogue_has_offers_from_every_corporation() {
        for company in ContractCompany::ALL {
            assert!(MACHINE_OFFERS.iter().any(|offer| offer.company == company));
        }
        assert!(MACHINE_OFFERS.iter().all(|offer| offer.price > 0));
        assert!(
            MACHINE_OFFERS
                .iter()
                .all(|offer| { offer.minimum_company_level <= crate::MAX_CORPORATION_LEVEL })
        );
        assert!(
            machine_offer(ItemId::RED_SHAFT_BORE)
                .is_some_and(|offer| offer.minimum_company_level >= 3)
        );
    }

    #[test]
    fn corporation_locked_offer_stays_unpurchased_until_the_required_level() {
        let mut system = DeliverySystem::default();
        let mut wallet = Wallet::new(10_000);
        assert_eq!(
            system.purchase(
                ItemId::RED_SHAFT_BORE,
                &mut wallet,
                CorporationProgress::default()
            ),
            Err(PurchaseError::CorporationLevelRequired)
        );
        assert_eq!(wallet.money(), 10_000);
        assert_eq!(system.pending_count(), 0);

        let level_three = CorporationProgress::from_experience([0, 0, 500]);
        assert_eq!(
            system.purchase(ItemId::RED_SHAFT_BORE, &mut wallet, level_three),
            Ok(())
        );
    }

    #[test]
    fn landed_crate_enters_inventory_when_player_is_nearby() {
        let registry = ItemRegistry::with_built_ins();
        let mut inventory = Inventory::new();
        let mut entities = EntityWorld::new();
        let crate_entity =
            spawn_delivery_crate(&mut entities, material(), ItemId::TURRET, [5.0, 6.0]);
        entities
            .get::<&mut Collider>(crate_entity)
            .unwrap()
            .on_ground = true;

        let collected = DeliverySystem::default().collect_nearby(
            &mut entities,
            &mut inventory,
            &registry,
            [5.0, 6.0],
        );

        assert_eq!(collected, 1);
        assert_eq!(inventory.slots()[0].unwrap().item(), ItemId::TURRET);
        assert!(!entities.contains(crate_entity));
    }
}
