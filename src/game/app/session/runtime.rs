use super::*;

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
        let terrain_renderer =
            TerrainRenderer::new(&mut gpu, demo_terrain_config(self.render_distance));
        let sky_renderer = SkyRenderer::new(&mut gpu);
        let mut sprite_renderer = SpriteRenderer::new(&mut gpu);
        let gui_renderer = GuiRenderer::new(&mut gpu);
        let bloom_renderer = BloomRenderer::new(&mut gpu);
        let player_body_texture = gpu.load_texture_from_file(
            include_bytes!("../../../../assets/player/player_body_atlas.png").to_vec(),
        );
        let player_body_frames = horizontal_sprite_frames(6);
        let player_body_material = sprite_renderer.create_material(
            &mut gpu,
            &terrain_renderer,
            player_body_texture,
            &player_body_frames,
        );
        let player_arm_texture = gpu.load_texture_from_file(
            include_bytes!("../../../../assets/player/player_arm_atlas.png").to_vec(),
        );
        let player_arm_frames = horizontal_sprite_frames(3);
        let player_arm_material = sprite_renderer.create_material(
            &mut gpu,
            &terrain_renderer,
            player_arm_texture,
            &player_arm_frames,
        );
        let lifeform_texture = gpu.load_texture_from_file(
            include_bytes!("../../../../assets/entities/player.png").to_vec(),
        );
        let lifeform_material = sprite_renderer.create_material(
            &mut gpu,
            &terrain_renderer,
            lifeform_texture,
            &[SpriteAtlasFrame::FULL],
        );
        let glowgnat_texture = gpu.load_texture_from_file(
            include_bytes!("../../../../assets/entities/glowgnat.png").to_vec(),
        );
        let glowgnat_material = sprite_renderer.create_material(
            &mut gpu,
            &terrain_renderer,
            glowgnat_texture,
            &[SpriteAtlasFrame::FULL],
        );
        let crate_texture = gpu.load_texture_from_file(
            include_bytes!("../../../../assets/furniture/furniture_with_power.png").to_vec(),
        );
        let delivery_crate_material = sprite_renderer.create_material(
            &mut gpu,
            &terrain_renderer,
            crate_texture,
            &horizontal_sprite_frames(13),
        );
        let dropped_item_texture = gpu.load_texture_from_file(
            include_bytes!("../../../../assets/gui/items_with_power.png").to_vec(),
        );
        let dropped_item_material = sprite_renderer.create_material(
            &mut gpu,
            &terrain_renderer,
            dropped_item_texture,
            &horizontal_sprite_frames(DROPPED_ITEM_ICON_FRAMES),
        );
        let throwable_texture = gpu.load_texture_from_file(
            include_bytes!("../../../../assets/entities/throwables.png").to_vec(),
        );
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
        let particle_texture = gpu.load_texture_from_file(
            include_bytes!("../../../../assets/entities/particles.png").to_vec(),
        );
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
        self.lifeform_materials =
            Some(LifeformMaterials::new(lifeform_material, glowgnat_material));
        self.specialist_material = Some(lifeform_material);
        self.delivery_crate_material = Some(delivery_crate_material);
        self.dropped_item_material = Some(dropped_item_material);
        self.effects_materials = Some(EffectsMaterials {
            projectile: projectile_material,
            particle: particle_material,
        });
        self.update_camera_zoom_limit();
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
                self.update_camera_zoom_limit();
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
                if code == KeyCode::Tab
                    && event.state == ElementState::Pressed
                    && !event.repeat
                    && !self.procurement_gui.is_open()
                    && !self.specialist_gui.is_open()
                {
                    self.toggle_world_map();
                    return;
                }
                if code == KeyCode::Escape && event.state == ElementState::Pressed {
                    self.handle_escape();
                    return;
                }
                if self.procurement_gui.is_open() || self.specialist_gui.is_open() {
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
                            } else if code == KeyCode::KeyX {
                                self.input.queue_interaction();
                            } else if code == KeyCode::KeyI {
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
                        if let Some(action) = self.contracts_gui.handle_click(
                            self.input.cursor_position(),
                            viewport,
                            self.contract_board.active(),
                            self.transmission_log.history(),
                        ) {
                            self.handle_contract_action(action);
                        }
                        return;
                    }
                    if self.screen == AppScreen::Map {
                        return;
                    }
                    if handle_incoming_transmission_click(
                        self.transmission_log.incoming(),
                        self.input.cursor_position(),
                        viewport,
                    ) {
                        self.transmission_log.dismiss_incoming();
                        return;
                    }
                    if incoming_transmission_captures_pointer(
                        self.transmission_log.incoming(),
                        self.input.cursor_position(),
                        viewport,
                    ) {
                        return;
                    }
                    if self.procurement_gui.is_open() {
                        let money = self.hud_snapshot().money;
                        if let Some(action) = self.procurement_gui.handle_click_with_catalogue(
                            self.input.cursor_position(),
                            viewport,
                            &self.contract_board,
                            self.corporation_progress,
                            money,
                        ) {
                            self.handle_procurement_action(action);
                        }
                        return;
                    }
                    if self.specialist_gui.is_open() {
                        if let Some(action) = self
                            .specialist_gui
                            .handle_click(self.input.cursor_position(), viewport)
                        {
                            self.handle_specialist_action(action);
                        }
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
                    if self.screen == AppScreen::Playing
                        && !self.procurement_gui.is_open()
                        && !self.specialist_gui.is_open()
                    {
                        let viewport = self.viewport();
                        if !self
                            .hud_gui
                            .captures_pointer(self.input.cursor_position(), viewport)
                            && !incoming_transmission_captures_pointer(
                                self.transmission_log.incoming(),
                                self.input.cursor_position(),
                                viewport,
                            )
                        {
                            self.input.press_secondary(&self.camera, viewport);
                        }
                    }
                }
                (MouseButton::Right, ElementState::Released) => {
                    if self.screen == AppScreen::Playing {
                        self.input.release_secondary();
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
                        if self.procurement_gui.is_open() {
                            let money = self.hud_snapshot().money;
                            self.procurement_gui.scroll_with_catalogue(
                                direction,
                                viewport,
                                &self.contract_board,
                                self.corporation_progress,
                                money,
                            );
                        } else if self.inventory_gui.scroll_crafting(
                            direction,
                            &self.inventory,
                            &self.item_registry,
                        ) {
                        } else if !self.specialist_gui.is_open()
                            && !self
                                .hud_gui
                                .captures_pointer(self.input.cursor_position(), viewport)
                        {
                            self.inventory
                                .cycle_hotbar(if direction > 0.0 { -1 } else { 1 });
                        }
                    } else if self.screen == AppScreen::Contracts {
                        self.contracts_gui.scroll(
                            direction,
                            self.contract_board.active().len(),
                            self.transmission_log.history().len(),
                        );
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
                let specialist_terminal_view = self
                    .specialist_system
                    .terminal_view(&self.world, self.procurement_gui.terminal());
                let advanced_targeting =
                    self.world.specialist_bonuses().advanced_turret_targeting();
                let hud_captures_pointer = self.screen == AppScreen::Playing
                    && (self
                        .hud_gui
                        .captures_pointer(self.input.cursor_position(), viewport)
                        || incoming_transmission_captures_pointer(
                            self.transmission_log.incoming(),
                            self.input.cursor_position(),
                            viewport,
                        ));
                if hud_captures_pointer {
                    self.input.release_primary();
                }
                let mut world_actions = Vec::new();
                if !simulation_paused {
                    handle_pointer_actions(
                        &mut self.input,
                        &self.camera,
                        viewport,
                        &mut self.inventory_gui,
                        &mut self.procurement_gui,
                        &mut self.specialist_gui,
                        &mut self.inventory,
                        &self.item_registry,
                        &mut self.world,
                        &self.power_system,
                        self.terrain_renderer
                            .as_mut()
                            .expect("terrain renderer exists"),
                        &mut self.dropped_item_system,
                        self.dropped_item_material,
                        &mut self.entities,
                        self.player,
                        self.effects_materials,
                        &mut world_actions,
                    );
                    for action in world_actions {
                        match action {
                            WorldAction::Mined(item) => {
                                self.contract_board.apply_mined(item, 1);
                                self.tutorial_program.record_extraction(item, 1);
                            }
                            WorldAction::Placed(object_type) => {
                                self.contract_board.apply_placement(object_type);
                            }
                            WorldAction::Sleep(bed) => self.try_sleep(bed),
                            WorldAction::TerminalUnpowered => {
                                self.save_notice = Some(SaveNotice {
                                    message: "TERMINAL OFFLINE - NO POWER",
                                    is_error: true,
                                    expires_at: Instant::now() + SAVE_NOTICE_DURATION,
                                });
                            }
                        }
                    }
                    self.tutorial_program.update(
                        elapsed,
                        &mut self.contract_board,
                        &mut self.transmission_log,
                        &mut self.delivery_system,
                        &self.world,
                        &self.power_system,
                        &self.inventory,
                        self.world.elevation_decimetres(player_position[1]),
                    );
                    self.refresh_contract_attention();
                }
                // Tutorial updates can schedule a delivery this frame. Capture
                // HUD state afterwards so its countdown appears immediately.
                let hud_snapshot = self.hud_snapshot();
                if !simulation_paused
                    && let (Some(materials), Some(dropped_item_material)) =
                        (self.effects_materials, self.dropped_item_material)
                {
                    self.effects_system.update(
                        &mut self.entities,
                        &mut self.world,
                        self.terrain_renderer
                            .as_mut()
                            .expect("terrain renderer exists"),
                        materials.particle,
                        DroppedItemContext::new(
                            &mut self.dropped_item_system,
                            dropped_item_material,
                            &self.item_registry,
                        ),
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
                for broken in nature.broken_tiles() {
                    if let Some((item, _)) =
                        block_definition(broken.tile).and_then(|block| block.mined_drop())
                    {
                        self.tutorial_program.record_drill_extraction(item, 1);
                    }
                }

                let target_preview = (!simulation_paused
                    && !hud_captures_pointer
                    && !self.procurement_gui.is_open()
                    && !self.specialist_gui.is_open())
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

                if let Some(material) = self.dropped_item_material {
                    for removed in nature.detached_objects().iter().cloned() {
                        self.dropped_item_system.spawn_removed_object(
                            &mut self.entities,
                            material,
                            &self.item_registry,
                            removed,
                        );
                    }
                }

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
                let subsurface_survey_status =
                    self.inventory_gui.open_container().and_then(|object| {
                        let world_object = self.world.object(object)?;
                        if world_object.object_type()
                            != deep_tek::FurnitureObject::SUBSURFACE_SURVEYOR
                        {
                            return None;
                        }
                        let active = world_object.is_active();
                        let powered = self.power_system.is_powered(object);
                        let survey = if active && powered {
                            let revision = self.world.foreground_revision();
                            if let Some(cached) = self.subsurface_survey_cache.filter(|cached| {
                                cached.object == object && cached.foreground_revision == revision
                            }) {
                                Some(cached.survey)
                            } else {
                                let survey = self.world.subsurface_survey(object)?;
                                self.subsurface_survey_cache = Some(CachedSubsurfaceSurvey {
                                    object,
                                    foreground_revision: revision,
                                    survey,
                                });
                                Some(survey)
                            }
                        } else {
                            None
                        };
                        Some(SubsurfaceSurveyStatus::new(powered, survey))
                    });
                if let Some(survey) = subsurface_survey_status.and_then(|status| status.survey) {
                    self.tutorial_program.record_scanner_result(survey);
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
                                            self.world.furniture_target_priority(object).map(
                                                |priority| {
                                                    if advanced_targeting {
                                                        priority
                                                    } else {
                                                        TargetPriority::Closest
                                                    }
                                                },
                                            ),
                                        )
                                        .with_laser_aim(self.world.laser_drill_aim(object))
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
                                        .with_subsurface_survey(subsurface_survey_status)
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
                        self.procurement_gui.queue(
                            gui,
                            ProcurementView::new(
                                &self.item_registry,
                                &self.delivery_system,
                                &self.contract_board,
                                &self.corporation_progress,
                                hud_snapshot.money,
                                &specialist_terminal_view,
                            ),
                            self.input.cursor_position(),
                            viewport,
                        );
                        let specialist_record = self
                            .specialist_gui
                            .specialist()
                            .and_then(|id| self.world.specialist(id));
                        self.specialist_gui.queue(
                            gui,
                            SpecialistView {
                                record: specialist_record,
                            },
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
                        queue_incoming_transmission(
                            gui,
                            self.transmission_log.incoming(),
                            self.input.cursor_position(),
                            viewport,
                        );
                        self.hud_gui
                            .queue_delivery_status(gui, hud_snapshot, viewport);
                    }
                    AppScreen::Paused => self.pause_menu.queue(
                        gui,
                        viewport,
                        self.input.cursor_position(),
                        self.render_distance,
                    ),
                    AppScreen::Contracts => self.contracts_gui.queue(
                        gui,
                        self.contract_board.active(),
                        self.transmission_log.history(),
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
