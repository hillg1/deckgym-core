use deckgym::{
    actions::{Action, SimpleAction},
    card_ids::CardId,
    models::{EnergyType, PlayedCard},
    test_support::get_initialized_game,
};

#[test]
fn test_minun_attack_no_plusle() {
    let mut game = get_initialized_game(0);
    let mut state = game.get_state_clone();

    state.set_board(
        vec![PlayedCard::from_id(CardId::B2165Minun)
            .with_energy(vec![EnergyType::Lightning])],
        vec![
            PlayedCard::from_id(CardId::A1001Bulbasaur),
            PlayedCard::from_id(CardId::A1001Bulbasaur),
        ],
    );
    state.current_player = 0;
    game.set_state(state);

    let attack_action = Action {
        actor: 0,
        action: SimpleAction::Attack(0),
        is_stack: false,
    };
    game.apply_action(&attack_action);

    let state = game.get_state_clone();
    assert_eq!(state.get_active(1).get_remaining_hp(), 40, "Base damage is 30 (70 - 30 = 40)");
    assert_eq!(state.in_play_pokemon[1][1].as_ref().unwrap().get_remaining_hp(), 70, "No bench damage");
}

#[test]
fn test_minun_attack_with_plusle() {
    let mut game = get_initialized_game(0);
    let mut state = game.get_state_clone();

    state.set_board(
        vec![
            PlayedCard::from_id(CardId::B2165Minun)
                .with_energy(vec![EnergyType::Lightning]),
            PlayedCard::from_id(CardId::B2052Plusle),
        ],
        vec![
            PlayedCard::from_id(CardId::A1001Bulbasaur),
            PlayedCard::from_id(CardId::A1001Bulbasaur),
        ],
    );
    state.current_player = 0;
    game.set_state(state);

    let attack_action = Action {
        actor: 0,
        action: SimpleAction::Attack(0),
        is_stack: false,
    };
    game.apply_action(&attack_action);

    let state = game.get_state_clone();
    assert_eq!(state.get_active(1).get_remaining_hp(), 40, "Base damage is 30 (70 - 30 = 40)");
    assert_eq!(state.in_play_pokemon[1][1].as_ref().unwrap().get_remaining_hp(), 60, "10 bench damage");
}
