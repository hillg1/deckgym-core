use deckgym::{
    actions::SimpleAction,
    card_ids::CardId,
    models::{EnergyType, PlayedCard},
    test_support::get_initialized_game,
};

#[test]
fn test_cherubi_attack_cost_reduction() {
    let mut game = get_initialized_game(0);
    let mut state = game.get_state_clone();

    let cherubi = PlayedCard::from_id(CardId::A4b026Cherubi);
    
    state.set_board(
        vec![cherubi],
        vec![PlayedCard::from_id(CardId::A1001Bulbasaur)],
    );
    state.current_player = 0;
    
    // Without a tool, Sweets Relay costs 2 Grass. With 1 Grass, it should not be usable.
    game.set_state(state.clone());
    let (_, actions) = game.get_state_clone().generate_possible_actions();
    let can_attack = actions.iter().any(|a| matches!(a.action, SimpleAction::Attack(0)));
    assert!(!can_attack, "Should not be able to attack without tool");
    
    // Attach a tool
    let tool_card = deckgym::database::get_card_by_enum(CardId::A3b067Leftovers);
    state.in_play_pokemon[0][0].as_mut().unwrap().attached_tool = Some(tool_card);

    // Now with a tool, it costs 1 Grass, so it is usable!
    game.set_state(state.clone());
    let (_, actions) = game.get_state_clone().generate_possible_actions();
    let can_attack = actions.iter().any(|a| matches!(a.action, SimpleAction::Attack(0)));
    assert!(can_attack, "Should be able to attack with tool attached");
}
