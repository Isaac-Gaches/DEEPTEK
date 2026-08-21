mod demo;

use deep_tek::{
    BATTERY_CAPACITY_MILLI, BatteryStatus, BloomRenderer, CargoLiftStatus, Contract,
    ContractsAction, ContractsGui, EffectsMaterials, EffectsSystem, Energy, FollowCamera,
    FurnitureGuiState, GuiRenderer, Health, HudAction, HudGui, HudSnapshot, Inventory,
    InventoryGui, ItemRegistry, ItemTransportSystem, Layer, LifeformId, LifeformSystem, MeterValue,
    NatureSimulationConfig, NatureUpdate, OrbitalExportSystem, PhysicsConfig, PlayerInput,
    PowerSystem, ProjectileSystem, SkyRenderer, SpriteAtlasFrame, SpriteRenderer,
    TerrainRenderConfig, TerrainRenderer, TileId, TilePos, TurretSystem, Wallet, World, WorldError,
    WorldGenerator, WorldMapGui, apply_export_to_contracts, built_in_contracts, entity_position,
    spawn_player, update_colliders, update_player_animation, update_players,
};
use demo::{
    InputState, PauseMenu, PauseMenuAction, WorldMenu, WorldMenuAction, handle_pointer_actions,
    hotbar_slot_for_key, is_jump_key, target_preview,
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
    lifeform_material: Option<Handle<Material>>,
    effects_materials: Option<EffectsMaterials>,
    world: World,
    entities: hecs::World,
    projectile_system: ProjectileSystem,
    turret_system: TurretSystem,
    item_transport_system: ItemTransportSystem,
    orbital_export_system: OrbitalExportSystem,
    power_system: PowerSystem,
    effects_system: EffectsSystem,
    lifeform_system: LifeformSystem,
    player: Option<Entity>,
    camera: FollowCamera,
    physics_config: PhysicsConfig,
    nature_config: NatureSimulationConfig,
    item_registry: ItemRegistry,
    inventory: Inventory,
    inventory_gui: InventoryGui,
    world_map_gui: WorldMapGui,
    hud_gui: HudGui,
    contracts_gui: ContractsGui,
    contracts: Vec<Contract>,
    screen: AppScreen,
    world_menu: WorldMenu,
    pause_menu: PauseMenu,
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
            lifeform_material: None,
            effects_materials: None,
            // Rendering and simulation stay paused on the world menu. This tiny
            // placeholder is replaced immediately after creating or loading a save.
            world: World::empty(1, 1, 0).expect("placeholder world dimensions are valid"),
            entities: hecs::World::new(),
            projectile_system: ProjectileSystem::default(),
            turret_system: TurretSystem::default(),
            item_transport_system: ItemTransportSystem::default(),
            orbital_export_system: OrbitalExportSystem::default(),
            power_system: PowerSystem::new(),
            effects_system: EffectsSystem::default(),
            lifeform_system: LifeformSystem::with_built_ins(),
            player: None,
            camera: FollowCamera::default(),
            physics_config: PhysicsConfig::default(),
            nature_config: NatureSimulationConfig::default(),
            item_registry,
            inventory,
            inventory_gui: InventoryGui::default(),
            world_map_gui: WorldMapGui::default(),
            hud_gui: HudGui,
            contracts_gui: ContractsGui,
            contracts: built_in_contracts(),
            screen: AppScreen::WorldMenu,
            world_menu: WorldMenu::default(),
            pause_menu: PauseMenu::default(),
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
        self.power_system
            .distribute(&mut self.world, time_of_day, power_elapsed);
        self.world
            .update_cargo_lifts(power_elapsed, &self.power_system, &self.item_registry);
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
        if let Some(player) = self.player {
            self.lifeform_system
                .update(&mut self.entities, player, elapsed);
        }
        update_colliders(
            &mut self.entities,
            &self.world,
            elapsed,
            self.physics_config,
        );
        update_player_animation(&mut self.entities, elapsed);
        if let Some(materials) = self.effects_materials {
            self.turret_system.update(
                &mut self.entities,
                &mut self.world,
                &self.power_system,
                materials.projectile,
                materials.particle,
                elapsed,
            );
        }
        self.projectile_system.update(&mut self.entities, elapsed);
        self.item_transport_system
            .update(&mut self.world, &self.item_registry, elapsed);
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
            let contract = apply_export_to_contracts(
                &mut self.contracts,
                shipment.stack.item(),
                u64::from(shipment.stack.quantity()),
            );
            income = income.saturating_add(contract.reward);
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
                        self.start_world(world, request.path);
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
                    Ok(world) => self.start_world(world, path),
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

    fn start_world(&mut self, world: World, path: PathBuf) {
        let saved_player_position = world.player_position();
        let saved_time_of_day = world.time_of_day();
        let gpu = self.gpu.as_mut().expect("renderer exists after resume");
        let terrain = self
            .terrain_renderer
            .as_mut()
            .expect("terrain renderer exists after resume");
        terrain.clear_meshes(gpu);
        terrain.mark_tile_dirty(0, 0, Layer::Foreground);

        self.world = world;
        self.world_map_gui.reset(&self.world);
        self.entities = hecs::World::new();
        self.projectile_system = ProjectileSystem::default();
        self.turret_system = TurretSystem::default();
        self.item_transport_system = ItemTransportSystem::default();
        self.orbital_export_system = OrbitalExportSystem::default();
        self.power_system = PowerSystem::new();
        self.effects_system = EffectsSystem::default();
        self.inventory = Inventory::starter(&self.item_registry);
        self.inventory_gui = InventoryGui::default();
        self.contracts = built_in_contracts();
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
        self.player = Some(player);
        let lifeform_height = self
            .lifeform_system
            .definition(LifeformId::WALKER)
            .expect("built-in walker definition exists")
            .collider_size[1];
        for offset in [-26_i64, -15, 15, 26] {
            let x = (spawn_position[0].round() as i64 + offset)
                .clamp(0, i64::from(self.world.width() - 1)) as u32;
            let position = find_grounded_spawn_position(&self.world, x, lifeform_height);
            self.lifeform_system
                .spawn(
                    &mut self.entities,
                    LifeformId::WALKER,
                    self.lifeform_material
                        .expect("lifeform material exists after resume"),
                    position,
                )
                .expect("built-in walker definition exists");
        }
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
            PauseMenuAction::SaveAndMainMenu => match self.save_active_world() {
                Ok(()) => {
                    self.active_world_path = None;
                    self.player = None;
                    self.entities = hecs::World::new();
                    self.inventory_gui = InventoryGui::default();
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

    fn handle_hud_action(&mut self, action: HudAction) {
        match action {
            HudAction::OpenContracts => self.screen = AppScreen::Contracts,
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
        }
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

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes().with_title("DeepTek");
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("failed to create window"),
        );
        let mut gpu = pollster::block_on(easy_gpu::Renderer::new(window.clone()))
            .clear_colour(0.08, 0.11, 0.16, 1.0);
        let terrain_renderer = TerrainRenderer::new(&mut gpu, demo_terrain_config());
        let sky_renderer = SkyRenderer::new(&mut gpu);
        let sprite_renderer = SpriteRenderer::new(&mut gpu);
        let gui_renderer = GuiRenderer::new(&mut gpu);
        let bloom_renderer = BloomRenderer::new(&mut gpu);
        let player_body_texture = gpu.load_texture_from_file(
            include_bytes!("../assets/player/player_body_atlas.png").to_vec(),
        );
        let player_body_frames = horizontal_sprite_frames(5);
        let player_body_material = sprite_renderer.create_material(
            &mut gpu,
            &terrain_renderer,
            player_body_texture,
            &player_body_frames,
        );
        let player_arm_texture = gpu.load_texture_from_file(
            include_bytes!("../assets/player/player_arm_atlas.png").to_vec(),
        );
        let player_arm_frames = horizontal_sprite_frames(3);
        let player_arm_material = sprite_renderer.create_material(
            &mut gpu,
            &terrain_renderer,
            player_arm_texture,
            &player_arm_frames,
        );
        let lifeform_texture =
            gpu.load_texture_from_file(include_bytes!("../assets/entities/player.png").to_vec());
        let lifeform_material = sprite_renderer.create_material(
            &mut gpu,
            &terrain_renderer,
            lifeform_texture,
            &[SpriteAtlasFrame::FULL],
        );
        let throwable_texture = gpu
            .load_texture_from_file(include_bytes!("../assets/entities/throwables.png").to_vec());
        let throwable_frames = [
            SpriteAtlasFrame::new([0.0, 0.0], [0.25, 1.0]),
            SpriteAtlasFrame::new([0.25, 0.0], [0.5, 1.0]),
            SpriteAtlasFrame::new([0.5, 0.0], [0.75, 1.0]),
            SpriteAtlasFrame::new([0.75, 0.0], [1.0, 1.0]),
        ];
        let projectile_material = sprite_renderer.create_material(
            &mut gpu,
            &terrain_renderer,
            throwable_texture,
            &throwable_frames,
        );
        let particle_texture =
            gpu.load_texture_from_file(include_bytes!("../assets/entities/particles.png").to_vec());
        let particle_material = sprite_renderer.create_material(
            &mut gpu,
            &terrain_renderer,
            particle_texture,
            &[
                SpriteAtlasFrame::new([0.25, 0.0], [0.5, 1.0]),
                SpriteAtlasFrame::new([0.5, 0.0], [0.75, 1.0]),
            ],
        );
        self.window = Some(window);
        self.gpu = Some(gpu);
        self.sky_renderer = Some(sky_renderer);
        self.terrain_renderer = Some(terrain_renderer);
        self.sprite_renderer = Some(sprite_renderer);
        self.gui_renderer = Some(gui_renderer);
        self.bloom_renderer = Some(bloom_renderer);
        self.player_body_material = Some(player_body_material);
        self.player_arm_material = Some(player_arm_material);
        self.lifeform_material = Some(lifeform_material);
        self.effects_materials = Some(EffectsMaterials {
            projectile: projectile_material,
            particle: particle_material,
        });
        let now = Instant::now();
        self.last_frame = now;
        self.next_frame = now;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                self.save_before_exit();
                event_loop.exit();
            }
            WindowEvent::Resized(size) if size.width > 0 && size.height > 0 => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize_surface(size);
                    if let Some(bloom) = &mut self.bloom_renderer {
                        bloom.resize(gpu, size.width, size.height);
                    }
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };
                if self.screen == AppScreen::WorldMenu {
                    if event.state == ElementState::Pressed {
                        let was_root = self.world_menu.is_root();
                        if let Some(action) =
                            self.world_menu.handle_key(code, event.text.as_deref())
                        {
                            self.handle_world_menu_action(action);
                        }
                        if code == KeyCode::Escape && was_root {
                            event_loop.exit();
                        }
                    }
                    return;
                }
                if code == KeyCode::Tab && event.state == ElementState::Pressed && !event.repeat {
                    self.toggle_world_map();
                    return;
                }
                if code == KeyCode::Escape && event.state == ElementState::Pressed {
                    self.handle_escape();
                    return;
                }
                if self.screen == AppScreen::Map {
                    match event.state {
                        ElementState::Pressed => {
                            let newly_pressed = self.input.press_key(code);
                            if newly_pressed
                                && matches!(
                                    code,
                                    KeyCode::KeyW
                                        | KeyCode::KeyA
                                        | KeyCode::KeyS
                                        | KeyCode::KeyD
                                        | KeyCode::ArrowUp
                                        | KeyCode::ArrowLeft
                                        | KeyCode::ArrowDown
                                        | KeyCode::ArrowRight
                                )
                            {
                                self.last_frame = Instant::now();
                            }
                            if matches!(code, KeyCode::Equal | KeyCode::NumpadAdd) {
                                self.world_map_gui.zoom_in();
                            } else if matches!(code, KeyCode::Minus | KeyCode::NumpadSubtract) {
                                self.world_map_gui.zoom_out();
                            }
                        }
                        ElementState::Released => self.input.release_key(code),
                    }
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                    return;
                }
                if matches!(self.screen, AppScreen::Paused | AppScreen::Contracts) {
                    return;
                }
                match event.state {
                    ElementState::Pressed => {
                        let newly_pressed = self.input.press_key(code);
                        if newly_pressed && is_jump_key(code) {
                            self.input.queue_jump();
                        }
                        if newly_pressed {
                            if let Some(slot) = hotbar_slot_for_key(code) {
                                self.inventory.select_hotbar(slot);
                            } else if code == KeyCode::KeyE {
                                self.inventory_gui
                                    .toggle(&mut self.inventory, &self.item_registry);
                            } else if matches!(code, KeyCode::Equal | KeyCode::NumpadAdd) {
                                self.camera.zoom_in();
                            } else if matches!(code, KeyCode::Minus | KeyCode::NumpadSubtract) {
                                self.camera.zoom_out();
                            }
                        }
                    }
                    ElementState::Released => {
                        self.input.release_key(code);
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input
                    .move_cursor([position.x as f32, position.y as f32]);
            }
            WindowEvent::MouseInput { state, button, .. } => match (button, state) {
                (MouseButton::Left, ElementState::Pressed) => {
                    let viewport = self.viewport();
                    if self.screen == AppScreen::WorldMenu {
                        if let Some(action) = self
                            .world_menu
                            .handle_click(self.input.cursor_position(), viewport)
                        {
                            self.handle_world_menu_action(action);
                        }
                        return;
                    }
                    if self.screen == AppScreen::Paused {
                        if let Some(action) = self
                            .pause_menu
                            .handle_click(self.input.cursor_position(), viewport)
                        {
                            self.handle_pause_menu_action(action);
                        }
                        return;
                    }
                    if self.screen == AppScreen::Contracts {
                        if let Some(ContractsAction::Close) = self.contracts_gui.handle_click(
                            self.input.cursor_position(),
                            viewport,
                            self.contracts.len(),
                        ) {
                            self.screen = AppScreen::Playing;
                            self.input.clear_focus();
                        }
                        return;
                    }
                    if self.screen == AppScreen::Map {
                        return;
                    }
                    if let Some(action) = self
                        .hud_gui
                        .handle_click(self.input.cursor_position(), viewport)
                    {
                        self.handle_hud_action(action);
                        return;
                    }
                    if self
                        .hud_gui
                        .captures_pointer(self.input.cursor_position(), viewport)
                    {
                        return;
                    }
                    self.input.press_primary(&self.camera, viewport);
                }
                (MouseButton::Left, ElementState::Released) => {
                    if self.screen == AppScreen::Playing {
                        self.input.release_primary();
                    }
                }
                (MouseButton::Right, ElementState::Pressed) => {
                    if self.screen == AppScreen::Playing {
                        let viewport = self.viewport();
                        if !self
                            .hud_gui
                            .captures_pointer(self.input.cursor_position(), viewport)
                        {
                            self.input.queue_secondary(&self.camera, viewport);
                        }
                    }
                }
                _ => {}
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let direction = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y.signum(),
                    MouseScrollDelta::PixelDelta(position) => (position.y as f32).signum(),
                };
                if direction != 0.0 {
                    if self.screen == AppScreen::WorldMenu {
                        let viewport = self.viewport();
                        self.world_menu.scroll(direction, viewport);
                    } else if self.screen == AppScreen::Playing {
                        let viewport = self.viewport();
                        if !self
                            .hud_gui
                            .captures_pointer(self.input.cursor_position(), viewport)
                        {
                            self.inventory
                                .cycle_hotbar(if direction > 0.0 { -1 } else { 1 });
                        }
                    }
                }
            }
            WindowEvent::Focused(false) => {
                self.focused = false;
                self.input.clear_focus();
            }
            WindowEvent::Focused(true) => {
                self.focused = true;
                let now = Instant::now();
                self.last_frame = now;
                self.next_frame = now;
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let elapsed_duration = now.duration_since(self.last_frame);
                let elapsed = elapsed_duration.as_secs_f32();
                self.last_frame = now;
                if self.screen == AppScreen::WorldMenu {
                    self.render_world_menu(elapsed);
                    return;
                }
                self.world_map_gui.advance(&self.world);
                if self.screen == AppScreen::Map {
                    self.render_world_map(elapsed);
                    return;
                }
                let simulation_paused =
                    matches!(self.screen, AppScreen::Paused | AppScreen::Contracts);
                if !simulation_paused {
                    let gpu = self.gpu.as_ref().expect("renderer exists after resume");
                    self.sky_renderer
                        .as_mut()
                        .expect("sky renderer exists after resume")
                        .update(gpu, elapsed);
                }
                let player_position = if simulation_paused {
                    self.player
                        .and_then(|player| entity_position(&self.entities, player))
                        .unwrap_or_else(|| self.camera.position())
                } else {
                    self.poll_autosave(now);
                    let time_of_day = self
                        .sky_renderer
                        .as_ref()
                        .expect("sky renderer exists after resume")
                        .time_of_day();
                    let position = self.update_entities(elapsed, time_of_day);
                    self.camera.follow(position, elapsed);
                    position
                };
                let viewport = self.viewport();
                let hud_snapshot = self.hud_snapshot();
                let hud_captures_pointer = self.screen == AppScreen::Playing
                    && self
                        .hud_gui
                        .captures_pointer(self.input.cursor_position(), viewport);
                if hud_captures_pointer {
                    self.input.release_primary();
                }
                if !simulation_paused {
                    handle_pointer_actions(
                        &mut self.input,
                        &self.camera,
                        viewport,
                        &mut self.inventory_gui,
                        &mut self.inventory,
                        &self.item_registry,
                        &mut self.world,
                        self.terrain_renderer
                            .as_mut()
                            .expect("terrain renderer exists"),
                        &mut self.entities,
                        self.player,
                        self.effects_materials,
                    );
                }
                if !simulation_paused && let Some(materials) = self.effects_materials {
                    self.effects_system.update(
                        &mut self.entities,
                        &mut self.world,
                        self.terrain_renderer
                            .as_mut()
                            .expect("terrain renderer exists"),
                        materials.particle,
                        elapsed,
                    );
                }
                let active_position = TilePos::new(
                    (player_position[0].floor().max(0.0) as u32).min(self.world.width() - 1),
                    (player_position[1].floor().max(0.0) as u32).min(self.world.height() - 1),
                );
                if !simulation_paused {
                    self.power_system.update(&self.world);
                }
                let nature = if simulation_paused {
                    NatureUpdate::default()
                } else {
                    self.world.update_nature_with_power(
                        elapsed_duration,
                        active_position,
                        self.nature_config,
                        &self.power_system,
                    )
                };
                if !simulation_paused {
                    self.power_system.update(&self.world);
                }

                let target_preview = (!simulation_paused && !hud_captures_pointer)
                    .then(|| {
                        target_preview(
                            &self.input,
                            &self.camera,
                            viewport,
                            &self.inventory_gui,
                            &self.inventory,
                            &self.item_registry,
                            &self.world,
                            &self.entities,
                            self.player,
                        )
                    })
                    .flatten();

                let gpu = self.gpu.as_mut().expect("renderer exists after resume");
                let terrain = self
                    .terrain_renderer
                    .as_mut()
                    .expect("terrain renderer exists after resume");
                let sky = self
                    .sky_renderer
                    .as_mut()
                    .expect("sky renderer exists after resume");
                for position in nature.changed_tiles() {
                    terrain.mark_tile_dirty(position.x, position.y, Layer::Foreground);
                }
                terrain.update_camera(
                    gpu,
                    self.camera.position(),
                    self.camera.vertical_tiles_visible(),
                );
                if !simulation_paused {
                    terrain.advance_visual_time(elapsed);
                }
                terrain.sync(gpu, &self.world, &self.power_system, player_position);
                if !simulation_paused && let Some(materials) = self.effects_materials {
                    self.effects_system.emit_laser_particles(
                        &mut self.entities,
                        terrain,
                        materials.particle,
                        elapsed,
                    );
                }
                let update_lighting = terrain.lighting_needs_refresh()
                    || self.last_lighting_update.elapsed() >= LIGHTING_UPDATE_INTERVAL;
                if update_lighting {
                    terrain.set_sky_light(gpu, sky.ambient_light());
                    let dynamic_lights = self.projectile_system.collect_lights(&self.entities);
                    terrain.update_lighting(gpu, &self.world, player_position, dynamic_lights);
                    self.last_lighting_update = now;
                }
                let gui = self
                    .gui_renderer
                    .as_mut()
                    .expect("GUI renderer exists after resume");
                gui.prepare(gpu, viewport);
                if let Some((pixel, size, tint)) = target_preview {
                    gui.queue_slot_rect(pixel, size, tint);
                }
                match self.screen {
                    AppScreen::Playing => {
                        self.inventory_gui.queue(
                            gui,
                            &self.inventory,
                            self.inventory_gui.open_container().and_then(|object| {
                                let active = self.world.object(object)?.is_active();
                                Some(
                                    FurnitureGuiState::new(self.world.container(object), active)
                                        .with_target_priority(
                                            self.world.furniture_target_priority(object),
                                        )
                                        .with_battery_status(
                                            self.world.battery_charge_milli(object).map(|stored| {
                                                BatteryStatus::new(stored, BATTERY_CAPACITY_MILLI)
                                            }),
                                        )
                                        .with_drill_depth(
                                            self.world
                                                .laser_bore_target(
                                                    object,
                                                    self.power_system.is_powered(object),
                                                )
                                                .map(|target| {
                                                    self.world.elevation_decimetres(target.y as f32)
                                                }),
                                        )
                                        .with_turret_kill_count(
                                            self.world.turret_kill_count(object),
                                        )
                                        .with_lift_status(
                                            self.world.cargo_lift_direction(object).map(
                                                |direction| {
                                                    CargoLiftStatus::new(
                                                        direction,
                                                        self.power_system.is_powered(object),
                                                    )
                                                },
                                            ),
                                        )
                                        .with_lift_station_configuration(
                                            self.world.lift_station_configuration(object),
                                        ),
                                )
                            }),
                            &self.item_registry,
                            self.input.cursor_position(),
                            viewport,
                        );
                        self.hud_gui.queue(
                            gui,
                            hud_snapshot,
                            self.input.cursor_position(),
                            viewport,
                        );
                        if let Some(notice) = &self.save_notice
                            && now < notice.expires_at
                        {
                            let text_width = GuiRenderer::text_width(notice.message, 2.0);
                            let centre = [viewport[0] * 0.5, 30.0];
                            gui.queue_rect(
                                centre,
                                [text_width + 28.0, 34.0],
                                [0.025, 0.055, 0.085, 0.94],
                            );
                            gui.queue_text(
                                notice.message,
                                [centre[0] - text_width * 0.5, centre[1] - 7.0],
                                2.0,
                                if notice.is_error {
                                    [1.0, 0.30, 0.25, 1.0]
                                } else {
                                    [0.45, 1.0, 0.58, 1.0]
                                },
                            );
                        }
                    }
                    AppScreen::Paused => {
                        self.pause_menu
                            .queue(gui, viewport, self.input.cursor_position())
                    }
                    AppScreen::Contracts => self.contracts_gui.queue(
                        gui,
                        &self.contracts,
                        self.input.cursor_position(),
                        viewport,
                    ),
                    AppScreen::Map => unreachable!("map rendering returned above"),
                    AppScreen::WorldMenu => unreachable!("world menu rendering returned above"),
                }
                let frame = gpu.begin_frame();
                if update_lighting {
                    terrain.compute_lighting(frame);
                }
                sky.draw(frame);
                terrain.draw(frame);
                self.sprite_renderer
                    .as_mut()
                    .expect("sprite renderer exists after resume")
                    .draw(frame, &self.entities);
                self.bloom_renderer
                    .as_ref()
                    .expect("bloom renderer exists after resume")
                    .queue(frame);
                gui.draw(frame);
                gpu.render();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            if !self.focused {
                event_loop.set_control_flow(ControlFlow::Wait);
                return;
            }
            let map_is_moving =
                self.input.horizontal_movement() != 0.0 || self.input.vertical_movement() != 0.0;
            if self.screen == AppScreen::Map && self.world_map_gui.is_ready() && !map_is_moving {
                event_loop.set_control_flow(ControlFlow::Wait);
                return;
            }
            let now = Instant::now();
            if now >= self.next_frame {
                window.request_redraw();
                self.next_frame = now + TARGET_FRAME_INTERVAL;
            }
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
        }
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
        horizontal_chunk_radius: 2,
        vertical_chunk_radius: 1,
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

fn main() -> Result<(), EventLoopError> {
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut App::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_closes_open_gameplay_ui_before_opening_pause() {
        let mut app = App::new();
        app.screen = AppScreen::Playing;
        assert!(
            app.inventory_gui
                .toggle(&mut app.inventory, &app.item_registry)
        );

        app.handle_escape();
        assert!(!app.inventory_gui.is_open());
        assert_eq!(app.screen, AppScreen::Playing);

        app.handle_escape();
        assert_eq!(app.screen, AppScreen::Paused);

        app.handle_escape();
        assert_eq!(app.screen, AppScreen::Playing);
    }

    #[test]
    fn escape_closes_contracts_without_opening_pause() {
        let mut app = App::new();
        app.screen = AppScreen::Contracts;

        app.handle_escape();

        assert_eq!(app.screen, AppScreen::Playing);
    }

    #[test]
    fn tab_toggles_the_world_map_and_escape_closes_it() {
        let mut app = App::new();
        app.screen = AppScreen::Playing;

        app.toggle_world_map();
        assert_eq!(app.screen, AppScreen::Map);

        app.toggle_world_map();
        assert_eq!(app.screen, AppScreen::Playing);

        app.toggle_world_map();
        app.handle_escape();
        assert_eq!(app.screen, AppScreen::Playing);
    }
}
