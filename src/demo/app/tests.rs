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
