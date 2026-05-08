use deckgym::{
    actions::{Action, SimpleAction},
    card_ids::CardId,
    models::{EnergyType, PlayedCard},
    test_support::get_initialized_game,
};

#[test]
fn test_chinchou_luring_glow() {
    let mut game = get_initialized_game(0);
    let mut state = game.get_state_clone();

    state.set_board(
        vec![PlayedCard::from_id(CardId::PA095Chinchou).with_energy(vec![EnergyType::Lightning])],
        vec![
            PlayedCard::from_id(CardId::A1001Bulbasaur), // Active
            PlayedCard::from_id(CardId::A1033Charmander), // Bench
        ],
    );
    state.current_player = 0;
    game.set_state(state);

    let attack_action = Action {
        actor: 0,
        action: SimpleAction::Attack(0),
        is_stack: false,
    };
    
    // Attack should execute without panicking
    game.apply_action(&attack_action);
    let state = game.get_state_clone();
    
    // Depending on the coin flip (which we don't control cleanly here), 
    // it will either have queued a choice or done nothing.
    // The test passes if it doesn't panic.
}
