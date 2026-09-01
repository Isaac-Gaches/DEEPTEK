use super::*;
use deep_tek::ContractCompany;

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

#[test]
fn collecting_a_completed_contract_deposits_its_reward_once() {
    let mut app = App::new();
    let player = app.entities.spawn((Wallet::new(0),));
    app.player = Some(player);
    app.contract_board.accept(0).unwrap();
    let progress = app.contract_board.active()[0].export_progress().unwrap();
    app.contract_board
        .apply_export(progress.item, progress.required);

    app.handle_contract_action(ContractsAction::CollectReward(0));

    assert_eq!(app.entities.get::<&Wallet>(player).unwrap().money(), 1_200);
    assert_eq!(
        app.corporation_progress
            .experience(ContractCompany::DeepTekIndustries),
        80
    );
    assert!(app.contract_board.active().is_empty());
    app.handle_contract_action(ContractsAction::CollectReward(0));
    assert_eq!(app.entities.get::<&Wallet>(player).unwrap().money(), 1_200);
    assert_eq!(
        app.corporation_progress
            .experience(ContractCompany::DeepTekIndustries),
        80
    );
}

#[test]
fn contracts_attention_reports_new_missions_and_new_rewards_until_opened() {
    let mut app = App::new();
    app.mark_contracts_viewed();
    app.contract_board
        .add_active(deep_tek::Contract::program_mission(
            deep_tek::ContractId::BreakingGround,
            "TEST MISSION",
            1,
            1,
            1,
        ));

    app.refresh_contract_attention();
    assert!(app.contracts_attention);

    app.handle_hud_action(HudAction::OpenContracts);
    assert!(!app.contracts_attention);

    assert!(
        app.contract_board
            .set_program_progress(deep_tek::ContractId::BreakingGround, 1, 1)
    );
    app.refresh_contract_attention();
    assert!(app.contracts_attention);
}

#[test]
fn application_lighting_allocation_follows_each_render_preset() {
    let medium = demo_terrain_config(RenderDistance::Medium);
    let high = demo_terrain_config(RenderDistance::High);
    assert_eq!(
        (medium.horizontal_chunk_radius, medium.vertical_chunk_radius),
        RenderDistance::Medium.chunk_radii()
    );
    assert_eq!(
        (high.horizontal_chunk_radius, high.vertical_chunk_radius),
        RenderDistance::High.chunk_radii()
    );
}
