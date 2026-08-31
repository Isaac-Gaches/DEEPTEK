mod runtime;

use super::{
    InputState, PauseMenu, PauseMenuAction, RenderDistance, WorldAction, WorldMenu,
    WorldMenuAction, handle_pointer_actions, hotbar_slot_for_key, is_jump_key, target_preview,
};
use deep_tek::{
    BATTERY_CAPACITY_MILLI, BatteryStatus, BloomRenderer, CargoLiftStatus, ContractBoard,
    ContractsAction, ContractsGui, CorporationProgress, DELIVERY_DROP_HEIGHT,
    DROPPED_ITEM_ICON_FRAMES, DeliverySystem, DroppedItemContext, DroppedItemSystem,
    EffectsMaterials, EffectsSystem, Energy, FollowCamera, FurnitureGuiState, GuiRenderer, Health,
    HudAction, HudGui, HudSnapshot, Inventory, InventoryGui, ItemRegistry, ItemTransportSystem,
    Layer, LifeformMaterials, LifeformSimulation, LifeformSpawnView, LifeformSystem,
    MachineProcessingSystem, MeterValue, NatureSimulationConfig, NatureUpdate, ObjectId,
    OrbitalExportSystem, PhysicsConfig, PlayerInput, PlayerState, PowerSystem, ProcurementAction,
    ProcurementGui, ProcurementView, ProjectileSystem, PurchaseError, SkyRenderer,
    SpecialistAction, SpecialistGui, SpecialistSystem, SpecialistView, SpikeDamageSystem,
    SpriteAtlasFrame, SpriteRenderer, SubsurfaceSurvey, SubsurfaceSurveyStatus, TargetPriority,
    TerrainRenderConfig, TerrainRenderer, TileId, TilePos, TransmissionLog, TurretSystem,
    TutorialProgram, Wallet, World, WorldError, WorldGenerator, WorldMapGui, assess_bed,
    block_definition, entity_position, handle_incoming_transmission_click,
    incoming_transmission_captures_pointer, is_daytime, queue_incoming_transmission, spawn_player,
    update_colliders, update_player_animation, update_players,
};
use easy_gpu::assets::Material;
use easy_gpu::assets_manager::Handle;
use hecs::Entity;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::error::EventLoopError;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

const LIGHTING_UPDATE_INTERVAL: Duration = Duration::from_millis(50);
const TARGET_FRAME_INTERVAL: Duration = Duration::from_nanos(16_666_667);
const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(60);
const SAVE_NOTICE_DURATION: Duration = Duration::from_secs(3);
const SLEEP_WAKE_TIME: f32 = 0.30;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppScreen {
    WorldMenu,
    Playing,
    Map,
    Paused,
    Contracts,
}

struct SaveNotice {
    message: &'static str,
    is_error: bool,
    expires_at: Instant,
}

#[derive(Clone, Copy)]
struct CachedSubsurfaceSurvey {
    object: ObjectId,
    foreground_revision: u64,
    survey: SubsurfaceSurvey,
}

type SaveJob = JoinHandle<Result<(), WorldError>>;

struct App {
    window: Option<Arc<Window>>,
    gpu: Option<easy_gpu::Renderer>,
    sky_renderer: Option<SkyRenderer>,
    terrain_renderer: Option<TerrainRenderer>,
    sprite_renderer: Option<SpriteRenderer>,
    gui_renderer: Option<GuiRenderer>,
    bloom_renderer: Option<BloomRenderer>,
    player_body_material: Option<Handle<Material>>,
    player_arm_material: Option<Handle<Material>>,
    lifeform_materials: Option<LifeformMaterials>,
    specialist_material: Option<Handle<Material>>,
    delivery_crate_material: Option<Handle<Material>>,
    dropped_item_material: Option<Handle<Material>>,
    effects_materials: Option<EffectsMaterials>,
    world: World,
    entities: hecs::World,
    projectile_system: ProjectileSystem,
    turret_system: TurretSystem,
    spike_damage_system: SpikeDamageSystem,
    item_transport_system: ItemTransportSystem,
    machine_processing_system: MachineProcessingSystem,
    orbital_export_system: OrbitalExportSystem,
    power_system: PowerSystem,
    effects_system: EffectsSystem,
    delivery_system: DeliverySystem,
    dropped_item_system: DroppedItemSystem,
    lifeform_system: LifeformSystem,
    lifeform_simulation: LifeformSimulation,
    specialist_system: SpecialistSystem,
    player: Option<Entity>,
    camera: FollowCamera,
    physics_config: PhysicsConfig,
    nature_config: NatureSimulationConfig,
    item_registry: ItemRegistry,
    inventory: Inventory,
    inventory_gui: InventoryGui,
    procurement_gui: ProcurementGui,
    specialist_gui: SpecialistGui,
    world_map_gui: WorldMapGui,
    hud_gui: HudGui,
    contracts_gui: ContractsGui,
    contract_board: ContractBoard,
    corporation_progress: CorporationProgress,
    transmission_log: TransmissionLog,
    tutorial_program: TutorialProgram,
    subsurface_survey_cache: Option<CachedSubsurfaceSurvey>,
    screen: AppScreen,
    world_menu: WorldMenu,
    pause_menu: PauseMenu,
    render_distance: RenderDistance,
    active_world_path: Option<PathBuf>,
    last_autosave: Instant,
    save_job: Option<SaveJob>,
    save_notice: Option<SaveNotice>,
    input: InputState,
    focused: bool,
    last_frame: Instant,
    next_frame: Instant,
    last_lighting_update: Instant,
}

impl App {
    fn new() -> Self {
        let item_registry = ItemRegistry::with_built_ins();
        let inventory = Inventory::starter(&item_registry);
        let now = Instant::now();
        Self {
            window: None,
            gpu: None,
            sky_renderer: None,
            terrain_renderer: None,
            sprite_renderer: None,
            gui_renderer: None,
            bloom_renderer: None,
            player_body_material: None,
            player_arm_material: None,
            lifeform_materials: None,
            specialist_material: None,
            delivery_crate_material: None,
            dropped_item_material: None,
            effects_materials: None,
            // Rendering and simulation stay paused on the world menu. This tiny
            // placeholder is replaced immediately after creating or loading a save.
            world: World::empty(1, 1, 0).expect("placeholder world dimensions are valid"),
            entities: hecs::World::new(),
            projectile_system: ProjectileSystem::default(),
            turret_system: TurretSystem::default(),
            spike_damage_system: SpikeDamageSystem::default(),
            item_transport_system: ItemTransportSystem::default(),
            machine_processing_system: MachineProcessingSystem::default(),
            orbital_export_system: OrbitalExportSystem::default(),
            power_system: PowerSystem::new(),
            effects_system: EffectsSystem::default(),
            delivery_system: DeliverySystem::default(),
            dropped_item_system: DroppedItemSystem::default(),
            lifeform_system: LifeformSystem::with_built_ins(),
            lifeform_simulation: LifeformSimulation::default(),
            specialist_system: SpecialistSystem::default(),
            player: None,
            camera: FollowCamera::default(),
            physics_config: PhysicsConfig::default(),
            nature_config: NatureSimulationConfig::default(),
            item_registry,
            inventory,
            inventory_gui: InventoryGui::default(),
            procurement_gui: ProcurementGui::default(),
            specialist_gui: SpecialistGui::default(),
            world_map_gui: WorldMapGui::default(),
            hud_gui: HudGui,
            contracts_gui: ContractsGui::default(),
            contract_board: ContractBoard::with_built_ins(),
            corporation_progress: CorporationProgress::default(),
            transmission_log: TransmissionLog::default(),
            tutorial_program: TutorialProgram::default(),
            subsurface_survey_cache: None,
            screen: AppScreen::WorldMenu,
            world_menu: WorldMenu::default(),
            pause_menu: PauseMenu::default(),
            render_distance: RenderDistance::default(),
            active_world_path: None,
            last_autosave: now,
            save_job: None,
            save_notice: None,
            input: InputState::default(),
            focused: true,
            last_frame: now,
            next_frame: now,
            last_lighting_update: now,
        }
    }

    fn update_entities(&mut self, elapsed: f32, time_of_day: f32) -> [f32; 2] {
        let power_elapsed = Duration::from_secs_f32(elapsed.clamp(0.0, 0.25));
        let specialist_bonuses = self.world.specialist_bonuses();
        self.power_system
            .distribute(&mut self.world, time_of_day, power_elapsed);
        self.world.update_cargo_lifts_with_speed(
            power_elapsed,
            &self.power_system,
            &self.item_registry,
            specialist_bonuses.lift_speed_percent(),
        );
        let horizontal = self.input.horizontal_movement();
        let vertical = self.input.vertical_movement();
        let jump_pressed = self.input.take_jump();
        update_players(
            &mut self.entities,
            &self.world,
            PlayerInput {
                horizontal,
                vertical,
                jump_pressed,
            },
            elapsed,
        );
        if let Some(player) = self.player
            && let Some(materials) = self.lifeform_materials
        {
            let player_position =
                entity_position(&self.entities, player).unwrap_or_else(|| self.camera.position());
            let viewport = self.viewport();
            let view = LifeformSpawnView::new(
                player_position,
                self.camera.screen_to_world([0.0, 0.0], viewport),
                self.camera.screen_to_world(viewport, viewport),
            );
            let lifeform_update = self.lifeform_simulation.update(
                &self.lifeform_system,
                &mut self.entities,
                player,
                &mut self.world,
                materials,
                view,
                Duration::from_secs_f32(elapsed.max(0.0)),
                self.physics_config,
            );
            for broken in lifeform_update.blocks_broken {
                self.terrain_renderer
                    .as_mut()
                    .expect("terrain renderer exists while simulating lifeforms")
                    .mark_tile_dirty(broken.position.x, broken.position.y, broken.layer);
            }
        }
        if let Some(player_position) = self
            .player
            .and_then(|player| entity_position(&self.entities, player))
            && let Some(material) = self.specialist_material
        {
            self.specialist_system.update(
                &mut self.world,
                &mut self.entities,
                material,
                player_position,
                Duration::from_secs_f32(elapsed.max(0.0)),
            );
        }
        if let Some(material) = self.delivery_crate_material {
            let drop_x = self
                .player
                .and_then(|player| entity_position(&self.entities, player))
                .map_or(self.camera.position()[0], |position| position[0]);
            // Resolve the surface only on the arrival frame. New worlds have a
            // very deep coordinate origin, so spawning at y=1 made an expired
            // countdown precede impact by minutes.
            let drop_y = if self
                .delivery_system
                .seconds_until_next()
                .is_some_and(|seconds| seconds <= elapsed.max(0.0))
            {
                let x = (drop_x.floor().max(0.0) as u32).min(self.world.width() - 1);
                (0..self.world.height())
                    .find(|&y| {
                        self.world
                            .tile(x, y, Layer::Foreground)
                            .is_ok_and(|tile| tile != TileId::EMPTY)
                    })
                    .map_or(1.0, |surface| {
                        (surface as f32 - DELIVERY_DROP_HEIGHT).max(1.0)
                    })
            } else {
                1.0
            };
            self.delivery_system.update_queue(
                &mut self.entities,
                material,
                drop_x,
                drop_y,
                self.world.width(),
                elapsed,
            );
        }
        update_colliders(
            &mut self.entities,
            &self.world,
            elapsed,
            self.physics_config,
        );
        self.spike_damage_system
            .update(&mut self.entities, &self.world, elapsed);
        if let Some(player_position) = self
            .player
            .and_then(|player| entity_position(&self.entities, player))
        {
            self.delivery_system.collect_nearby(
                &mut self.entities,
                &mut self.inventory,
                &self.item_registry,
                player_position,
            );
            self.dropped_item_system.update(
                &mut self.entities,
                &mut self.inventory,
                &self.item_registry,
                player_position,
                elapsed,
            );
        }
        update_player_animation(&mut self.entities, elapsed);
        if let Some(materials) = self.effects_materials {
            self.turret_system.update_with_bonuses(
                &mut self.entities,
                &mut self.world,
                &self.power_system,
                materials.projectile,
                materials.particle,
                elapsed,
                specialist_bonuses,
            );
        }
        self.projectile_system.update(&mut self.entities, elapsed);
        let transport_update = self.item_transport_system.update_with_speed(
            &mut self.world,
            &self.item_registry,
            elapsed,
            specialist_bonuses.conveyor_speed_percent(),
        );
        self.tutorial_program
            .record_automatic_processor_transfer(transport_update.processor_items_received);
        let processing_update = self.machine_processing_system.update_with_speed(
            &mut self.world,
            &self.item_registry,
            &self.power_system,
            power_elapsed,
            specialist_bonuses.processing_speed_percent(),
        );
        self.tutorial_program
            .record_iron_processing(processing_update.iron_ore_processed);
        let shipments = self
            .orbital_export_system
            .update(
                &mut self.world,
                &self.item_registry,
                &self.power_system,
                elapsed,
            )
            .to_vec();
        let mut income = 0_u64;
        for shipment in shipments {
            if let Some(materials) = self.effects_materials
                && let Some(origin) = self.world.object(shipment.launcher).map(|launcher| {
                    let [width, _] = launcher.size();
                    [
                        launcher.anchor().x as f32 + (f32::from(width) - 1.0) * 0.5,
                        launcher.anchor().y as f32 - 0.45,
                    ]
                })
            {
                self.effects_system.emit_export_launch_sparks(
                    &mut self.entities,
                    materials.particle,
                    origin,
                );
            }
            income = income.saturating_add(shipment.proceeds);
            self.contract_board
                .apply_export(shipment.stack.item(), u64::from(shipment.stack.quantity()));
            self.tutorial_program
                .record_export(shipment.stack.item(), u64::from(shipment.stack.quantity()));
        }
        if income > 0
            && let Some(player) = self.player
            && let Ok(mut wallet) = self.entities.get::<&mut Wallet>(player)
        {
            wallet.deposit(income);
        }
        self.player
            .and_then(|player| entity_position(&self.entities, player))
            .unwrap_or_else(|| self.camera.position())
    }

    fn viewport(&self) -> [f32; 2] {
        self.gpu
            .as_ref()
            .map_or([1.0, 1.0], |gpu| [gpu.width() as f32, gpu.height() as f32])
    }

    fn handle_world_menu_action(&mut self, action: WorldMenuAction) {
        match action {
            WorldMenuAction::Create(request) => {
                self.world_menu.set_status("CREATING WORLD", false);
                if let Err(error) = self.world_menu.ensure_directory() {
                    eprintln!("failed to create the world directory: {error}");
                    self.world_menu.set_status("CREATE FAILED", true);
                    return;
                }
                let generated = WorldGenerator::new(request.seed)
                    .with_threads(demo_worker_threads())
                    .generate(request.width, request.height);
                match generated {
                    Ok(mut world) => {
                        if let Err(error) = world.set_name(request.name) {
                            eprintln!("failed to set the new world name: {error}");
                            self.world_menu.set_status("CREATE FAILED", true);
                            return;
                        }
                        if let Err(error) = world.save_with_threads(&request.path, 2) {
                            eprintln!("failed to save the new world: {error}");
                            self.world_menu.set_status("CREATE FAILED", true);
                            return;
                        }
                        self.start_world(world, request.path, true);
                    }
                    Err(error) => {
                        eprintln!("failed to generate a new world: {error}");
                        self.world_menu.set_status("CREATE FAILED", true);
                    }
                }
            }
            WorldMenuAction::Load(path) => {
                self.world_menu.set_status("LOADING WORLD", false);
                match World::load_with_threads(&path, demo_worker_threads()) {
                    Ok(world) => self.start_world(world, path, false),
                    Err(error) => {
                        eprintln!("failed to load world {}: {error}", path.display());
                        self.world_menu.set_status("LOAD FAILED", true);
                    }
                }
            }
            WorldMenuAction::Delete(path) => match self.world_menu.delete_world(&path) {
                Ok(()) => self.world_menu.set_status("WORLD DELETED", false),
                Err(error) => {
                    eprintln!("failed to delete world {}: {error}", path.display());
                    self.world_menu.set_status("DELETE FAILED", true);
                }
            },
        }
    }

    fn start_world(&mut self, world: World, path: PathBuf, is_new_world: bool) {
        let saved_player_position = world.player_position();
        let saved_player_state = world.player_state().cloned();
        let saved_time_of_day = world.time_of_day();
        let gpu = self.gpu.as_mut().expect("renderer exists after resume");
        let terrain = self
            .terrain_renderer
            .as_mut()
            .expect("terrain renderer exists after resume");
        terrain.clear_meshes(gpu);
        terrain.mark_tile_dirty(0, 0, Layer::Foreground);

        self.world = world;
        self.subsurface_survey_cache = None;
        self.world_map_gui.reset(&self.world);
        self.entities = hecs::World::new();
        self.projectile_system = ProjectileSystem::default();
        self.turret_system = TurretSystem::default();
        self.spike_damage_system = SpikeDamageSystem::default();
        self.item_transport_system = ItemTransportSystem::default();
        self.machine_processing_system = MachineProcessingSystem::default();
        self.orbital_export_system = OrbitalExportSystem::default();
        self.power_system = PowerSystem::new();
        self.effects_system = EffectsSystem::default();
        self.delivery_system = saved_player_state
            .as_ref()
            .map_or_else(DeliverySystem::default, |state| {
                state.delivery_system().clone()
            });
        self.dropped_item_system = DroppedItemSystem::default();
        self.lifeform_simulation = LifeformSimulation::default();
        self.specialist_system.reset();
        self.inventory = saved_player_state.as_ref().map_or_else(
            || Inventory::starter(&self.item_registry),
            |state| state.inventory().clone(),
        );
        self.inventory_gui = InventoryGui::default();
        if let Some(state) = saved_player_state.as_ref() {
            self.inventory_gui
                .restore_cursor_stack(state.cursor_stack());
        }
        self.procurement_gui = ProcurementGui::default();
        self.specialist_gui = SpecialistGui::default();
        self.contracts_gui = ContractsGui::default();
        self.contract_board = saved_player_state
            .as_ref()
            .map_or_else(ContractBoard::with_built_ins, |state| {
                state.contract_board().clone()
            });
        self.corporation_progress = saved_player_state
            .as_ref()
            .map_or_else(CorporationProgress::default, |state| {
                state.corporation_progress()
            });
        self.transmission_log = saved_player_state
            .as_ref()
            .map_or_else(TransmissionLog::default, |state| {
                state.transmission_log().clone()
            });
        self.tutorial_program = saved_player_state.as_ref().map_or_else(
            || {
                if is_new_world {
                    TutorialProgram::for_new_world()
                } else {
                    TutorialProgram::default()
                }
            },
            |state| state.tutorial_program(),
        );
        self.input.clear_focus();

        self.sky_renderer
            .as_mut()
            .expect("sky renderer exists after resume")
            .set_time_of_day(gpu, saved_time_of_day);

        let spawn_position = saved_player_position
            .unwrap_or_else(|| find_spawn_position(&self.world, self.world.width() / 2));
        let player = spawn_player(
            &mut self.entities,
            self.player_body_material
                .expect("player body material exists after resume"),
            self.player_arm_material
                .expect("player arm material exists after resume"),
            spawn_position,
        );
        if let Some(state) = saved_player_state
            && let Ok(mut health) = self.entities.get::<&mut Health>(player)
        {
            *health = Health::with_current(state.health_current(), state.health_maximum())
                .expect("saved player health was validated while loading");
        }
        self.player = Some(player);
        self.camera.snap_to(spawn_position);
        self.active_world_path = Some(path);
        self.screen = AppScreen::Playing;
        self.world_menu.show_root();
        self.pause_menu.clear_error();
        self.world_menu.clear_status();
        let now = Instant::now();
        self.last_frame = now;
        self.last_autosave = now;
        self.last_lighting_update = now;
        self.save_notice = Some(SaveNotice {
            message: "WORLD READY",
            is_error: false,
            expires_at: now + SAVE_NOTICE_DURATION,
        });
    }

    fn capture_session_metadata(&mut self) {
        self.specialist_system
            .sync_to_world(&self.entities, &mut self.world);
        self.world.prune_orphaned_specialists();
        if let Some(position) = self
            .player
            .and_then(|player| entity_position(&self.entities, player))
            && let Err(error) = self.world.set_player_position(Some(position))
        {
            eprintln!("failed to capture player position: {error}");
        }
        if let Some(sky) = &self.sky_renderer {
            self.world
                .set_time_of_day(sky.time_of_day())
                .expect("sky renderer always exposes a finite normalized time");
        }
        if let Some(player) = self.player
            && let Ok(health) = self.entities.get::<&Health>(player)
        {
            let state =
                PlayerState::new(health.current(), health.maximum(), self.inventory.clone())
                    .expect("live player health is valid")
                    .with_cursor_stack(self.inventory_gui.cursor_stack())
                    .with_corporation_progress(self.corporation_progress)
                    .with_mission_state(
                        self.contract_board.clone(),
                        self.transmission_log.clone(),
                        self.tutorial_program,
                        self.delivery_system.clone(),
                    );
            self.world.set_player_state(Some(state));
        }
    }

    fn try_sleep(&mut self, bed: ObjectId) {
        let (message, is_error) =
            if !assess_bed(&self.world, bed).is_some_and(deep_tek::HouseRequirements::is_valid) {
                ("BED REQUIRES A SUITABLE HOUSE", true)
            } else {
                let current_time = self
                    .sky_renderer
                    .as_ref()
                    .expect("sky renderer exists while playing")
                    .time_of_day();
                if is_daytime(current_time) {
                    ("YOU CAN ONLY SLEEP AT NIGHT", true)
                } else {
                    let gpu = self.gpu.as_ref().expect("renderer exists while playing");
                    self.sky_renderer
                        .as_mut()
                        .expect("sky renderer exists while playing")
                        .set_time_of_day(gpu, SLEEP_WAKE_TIME);
                    self.world
                        .set_time_of_day(SLEEP_WAKE_TIME)
                        .expect("sleep wake time is finite");
                    ("SLEPT UNTIL MORNING", false)
                }
            };
        self.save_notice = Some(SaveNotice {
            message,
            is_error,
            expires_at: Instant::now() + SAVE_NOTICE_DURATION,
        });
    }

    fn poll_autosave(&mut self, now: Instant) {
        if self.save_job.as_ref().is_some_and(JoinHandle::is_finished) {
            let result = self
                .save_job
                .take()
                .expect("finished save job exists")
                .join();
            let (message, is_error) = match result {
                Ok(Ok(())) => ("WORLD SAVED", false),
                Ok(Err(error)) => {
                    eprintln!("autosave failed: {error}");
                    ("SAVE FAILED", true)
                }
                Err(_) => {
                    eprintln!("autosave worker panicked");
                    ("SAVE FAILED", true)
                }
            };
            self.save_notice = Some(SaveNotice {
                message,
                is_error,
                expires_at: now + SAVE_NOTICE_DURATION,
            });
        }

        if self.save_job.is_none()
            && now.duration_since(self.last_autosave) >= AUTOSAVE_INTERVAL
            && let Some(path) = self.active_world_path.clone()
        {
            self.capture_session_metadata();
            let snapshot = self.world.clone();
            self.save_job = Some(std::thread::spawn(move || {
                snapshot.save_with_threads(path, 2)
            }));
            self.last_autosave = now;
        }
    }

    fn save_active_world(&mut self) -> Result<(), WorldError> {
        if !matches!(
            self.screen,
            AppScreen::Playing | AppScreen::Map | AppScreen::Paused | AppScreen::Contracts
        ) {
            return Ok(());
        }
        if let Some(job) = self.save_job.take() {
            let _ = job.join();
        }
        self.capture_session_metadata();
        if let Some(path) = &self.active_world_path {
            self.world.save_with_threads(path, 2)?;
        }
        Ok(())
    }

    fn save_before_exit(&mut self) {
        if let Err(error) = self.save_active_world() {
            eprintln!("failed to save world before exit: {error}");
        }
    }

    fn handle_pause_menu_action(&mut self, action: PauseMenuAction) {
        match action {
            PauseMenuAction::Resume => {
                self.screen = AppScreen::Playing;
                self.pause_menu.clear_error();
                self.input.clear_focus();
            }
            PauseMenuAction::Settings => self.pause_menu.show_settings(),
            PauseMenuAction::Back => self.pause_menu.show_root(),
            PauseMenuAction::SetRenderDistance(distance) => {
                self.render_distance = distance;
                let (horizontal, vertical) = distance.chunk_radii();
                if let Some(terrain) = &mut self.terrain_renderer {
                    terrain.set_render_distance(horizontal, vertical);
                }
                self.update_camera_zoom_limit();
            }
            PauseMenuAction::SaveAndMainMenu => match self.save_active_world() {
                Ok(()) => {
                    self.active_world_path = None;
                    self.player = None;
                    self.entities = hecs::World::new();
                    self.inventory_gui = InventoryGui::default();
                    self.procurement_gui = ProcurementGui::default();
                    self.specialist_gui = SpecialistGui::default();
                    self.delivery_system = DeliverySystem::default();
                    self.input.clear_focus();
                    self.screen = AppScreen::WorldMenu;
                    self.world_menu.show_root();
                    if let Err(error) = self.world_menu.refresh() {
                        eprintln!("failed to refresh saved worlds: {error}");
                        self.world_menu.set_status("WORLD LIST ERROR", true);
                    } else {
                        self.world_menu.set_status("WORLD SAVED", false);
                    }
                }
                Err(error) => {
                    eprintln!("failed to save world: {error}");
                    self.pause_menu.set_save_failed();
                }
            },
        }
    }

    fn update_camera_zoom_limit(&mut self) {
        let viewport = self.viewport();
        let aspect = viewport[0] / viewport[1].max(1.0);
        let (horizontal, vertical) = self.render_distance.chunk_radii();
        let chunk_size = deep_tek::CHUNK_SIZE as u32;
        let safe_horizontal = ((horizontal * 2 * chunk_size - 8) as f32 / aspect).max(12.0);
        let safe_vertical = (vertical * 2 * chunk_size - 8) as f32;
        self.camera.set_maximum_vertical_tiles_visible(
            self.render_distance
                .maximum_zoom()
                .min(safe_horizontal)
                .min(safe_vertical),
        );
    }

    fn handle_hud_action(&mut self, action: HudAction) {
        match action {
            HudAction::OpenContracts => {
                self.contracts_gui.show_contracts();
                self.screen = AppScreen::Contracts;
            }
            HudAction::Pause => {
                self.screen = AppScreen::Paused;
                self.pause_menu.clear_error();
            }
        }
        self.input.clear_focus();
    }

    fn handle_escape(&mut self) {
        match self.screen {
            AppScreen::Playing if self.inventory_gui.is_open() => {
                if !self
                    .inventory_gui
                    .toggle(&mut self.inventory, &self.item_registry)
                {
                    // Closing remains authoritative even in the rare case that
                    // every inventory slot filled while an item was held.
                    self.inventory_gui.dismiss();
                }
            }
            AppScreen::Playing if self.procurement_gui.is_open() => {
                self.procurement_gui.dismiss();
            }
            AppScreen::Playing if self.specialist_gui.is_open() => {
                self.specialist_gui.dismiss();
            }
            AppScreen::Playing => {
                self.screen = AppScreen::Paused;
                self.pause_menu.clear_error();
            }
            AppScreen::Paused => self.handle_pause_menu_action(PauseMenuAction::Resume),
            AppScreen::Contracts => self.screen = AppScreen::Playing,
            AppScreen::Map => self.screen = AppScreen::Playing,
            AppScreen::WorldMenu => return,
        }
        self.input.clear_focus();
    }

    fn toggle_world_map(&mut self) {
        match self.screen {
            AppScreen::Playing => {
                self.inventory_gui.dismiss();
                self.procurement_gui.dismiss();
                self.specialist_gui.dismiss();
                self.world_map_gui.invalidate_furniture();
                self.screen = AppScreen::Map;
            }
            AppScreen::Map => self.screen = AppScreen::Playing,
            AppScreen::WorldMenu | AppScreen::Paused | AppScreen::Contracts => return,
        }
        self.input.clear_focus();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn hud_snapshot(&self) -> HudSnapshot {
        let Some(player) = self.player else {
            return HudSnapshot::default();
        };
        let health = self
            .entities
            .get::<&Health>(player)
            .ok()
            .map(|health| *health)
            .unwrap_or_else(|| Health::new(0));
        let energy = self
            .entities
            .get::<&Energy>(player)
            .ok()
            .map(|energy| *energy)
            .unwrap_or_else(|| Energy::new(0));
        let money = self
            .entities
            .get::<&Wallet>(player)
            .ok()
            .map_or(0, |wallet| wallet.money());
        let depth_decimetres = entity_position(&self.entities, player)
            .map_or(0, |position| self.world.elevation_decimetres(position[1]));
        HudSnapshot {
            health: MeterValue::new(health.current(), health.maximum()),
            energy: MeterValue::new(energy.current(), energy.maximum()),
            money,
            depth_decimetres,
            delivery_seconds: self
                .delivery_system
                .seconds_until_next()
                .map(|seconds| seconds.ceil().min(u32::MAX as f32) as u32),
            delivery_count: self.delivery_system.pending_count(),
        }
    }

    fn handle_procurement_action(&mut self, action: ProcurementAction) {
        if action != ProcurementAction::Close
            && !self
                .procurement_gui
                .terminal()
                .is_some_and(|terminal| self.power_system.is_powered(terminal))
        {
            self.procurement_gui.dismiss();
            self.save_notice = Some(SaveNotice {
                message: "TERMINAL OFFLINE - NO POWER",
                is_error: true,
                expires_at: Instant::now() + SAVE_NOTICE_DURATION,
            });
            return;
        }
        match action {
            ProcurementAction::Close => self.procurement_gui.dismiss(),
            ProcurementAction::Buy(item) => {
                let result = self
                    .player
                    .and_then(|player| self.entities.get::<&mut Wallet>(player).ok())
                    .map_or(Err(PurchaseError::ItemNotOffered), |mut wallet| {
                        self.delivery_system.purchase(item, &mut wallet)
                    });
                if result.is_ok() {
                    self.tutorial_program.record_purchase();
                }
                self.procurement_gui.set_purchase_result(result);
            }
            ProcurementAction::AcceptContract(index) => {
                let result = self.contract_board.accept(index);
                self.procurement_gui
                    .set_contract_result(result, self.contract_board.available().len());
            }
            ProcurementAction::RecruitSpecialist(id) => {
                let result = self
                    .procurement_gui
                    .terminal()
                    .ok_or(deep_tek::RecruitSpecialistError::InvalidTerminal)
                    .and_then(|terminal| {
                        self.specialist_system
                            .recruit(&mut self.world, terminal, id)
                    });
                self.procurement_gui.set_specialist_result(result);
            }
        }
        self.input.clear_focus();
    }

    fn handle_specialist_action(&mut self, action: SpecialistAction) {
        match action {
            SpecialistAction::Close => self.specialist_gui.dismiss(),
            SpecialistAction::OpenDetails => self.specialist_gui.open_details(),
        }
        self.input.clear_focus();
    }

    fn handle_contract_action(&mut self, action: ContractsAction) {
        match action {
            ContractsAction::Close => self.screen = AppScreen::Playing,
            ContractsAction::CollectReward(index) => {
                if let Some(player) = self.player
                    && let Ok(mut wallet) = self.entities.get::<&mut Wallet>(player)
                    && let Ok(reward) = self.contract_board.claim_reward(index)
                {
                    wallet.deposit(reward.money);
                    self.corporation_progress
                        .award(reward.company, reward.experience);
                }
            }
        }
        self.input.clear_focus();
    }

    fn render_world_menu(&mut self, elapsed: f32) {
        let viewport = self.viewport();
        let gpu = self.gpu.as_mut().expect("renderer exists after resume");
        let sky = self
            .sky_renderer
            .as_mut()
            .expect("sky renderer exists after resume");
        sky.update(gpu, elapsed);
        let gui = self
            .gui_renderer
            .as_mut()
            .expect("GUI renderer exists after resume");
        gui.prepare(gpu, viewport);
        self.world_menu
            .queue(gui, viewport, self.input.cursor_position());
        let frame = gpu.begin_frame();
        sky.draw(frame);
        self.bloom_renderer
            .as_ref()
            .expect("bloom renderer exists after resume")
            .queue(frame);
        gui.draw(frame);
        gpu.render();
    }

    fn render_world_map(&mut self, elapsed: f32) {
        let viewport = self.viewport();
        let player_position = self
            .player
            .and_then(|player| entity_position(&self.entities, player))
            .unwrap_or_else(|| self.camera.position());
        let camera_position = self.camera.position();
        let camera_vertical_tiles = self.camera.vertical_tiles_visible();
        self.world_map_gui.navigate(
            [
                self.input.horizontal_movement(),
                self.input.vertical_movement(),
            ],
            elapsed,
        );
        let gpu = self.gpu.as_mut().expect("renderer exists after resume");
        let gui = self
            .gui_renderer
            .as_mut()
            .expect("GUI renderer exists after resume");
        gui.prepare(gpu, viewport);
        self.world_map_gui.upload_if_needed(gpu, gui, viewport);
        self.world_map_gui.queue(
            gui,
            &self.world,
            &self.item_registry,
            player_position,
            (camera_position, camera_vertical_tiles),
            viewport,
        );
        let frame = gpu.begin_frame();
        gui.draw(frame);
        gpu.render();
    }
}

fn horizontal_sprite_frames(count: u32) -> Vec<SpriteAtlasFrame> {
    let frame_width = 1.0 / count.max(1) as f32;
    (0..count.max(1))
        .map(|frame| {
            SpriteAtlasFrame::new(
                [frame as f32 * frame_width, 0.0],
                [(frame + 1) as f32 * frame_width, 1.0],
            )
        })
        .collect()
}

fn demo_terrain_config() -> TerrainRenderConfig {
    TerrainRenderConfig {
        // The 55-tile-high camera needs only about 100 horizontal tiles at 16:9.
        // Five by three 64-tile chunks leaves a generous safety margin while making
        // the multi-pass lightmap over five times smaller than the library default.
        // Lighting is allocated once for the highest settings preset. The active
        // mesh-streaming radius starts at Medium and can be changed immediately.
        horizontal_chunk_radius: 3,
        vertical_chunk_radius: 2,
        mesh_layer_budget_per_frame: 8,
        worker_threads: demo_worker_threads(),
        ..TerrainRenderConfig::default()
    }
}

fn demo_worker_threads() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .min(4)
}

fn find_spawn_position(world: &World, x: u32) -> [f32; 2] {
    find_grounded_spawn_position(world, x, 2.7)
}

fn find_grounded_spawn_position(world: &World, x: u32, collider_height: f32) -> [f32; 2] {
    let x = x.min(world.width() - 1);
    let surface = (0..world.height())
        .find(|&y| {
            world
                .tile(x, y, Layer::Foreground)
                .is_ok_and(|tile| tile != TileId::EMPTY)
        })
        .unwrap_or(world.height() - 1);
    let half_height = collider_height * 0.5;
    [
        x as f32,
        (surface as f32 - 0.5 - half_height).max(half_height + 0.01),
    ]
}

pub(crate) fn run() -> Result<(), EventLoopError> {
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut App::new())
}

#[cfg(test)]
mod tests;
