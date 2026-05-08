use deckgym::{
    actions::{Action, SimpleAction},
    card_ids::CardId,
    models::{EnergyType, PlayedCard},
    test_support::get_initialized_game,
};

#[test]
fn test_wishiwashi_ex_gathering_attack_no_bench() {
    let mut game = get_initialized_game(0);
    let mut state = game.get_state_clone();

    state.set_board(
        vec![PlayedCard::from_id(CardId::A4b124WishiwashiEx)
            .with_energy(vec![EnergyType::Water, EnergyType::Water])],
        vec![PlayedCard::from_id(CardId::A1001Bulbasaur)],
    );
    state.current_player = 0;
    game.set_state(state);

    let attack_action = Action {
        actor: 0,
        action: SimpleAction::Attack(0),
        is_stack: false,
    };
    game.apply_action(&attack_action);

    let opponent_hp = game.get_state_clone().get_active(1).get_remaining_hp();
    assert_eq!(opponent_hp, 40, "Base damage is 30 (70 - 30 = 40)");
}

#[test]
fn test_wishiwashi_ex_gathering_attack_with_bench() {
    let mut game = get_initialized_game(0);
    let mut state = game.get_state_clone();

    state.set_board(
        vec![
            PlayedCard::from_id(CardId::A4b124WishiwashiEx)
                .with_energy(vec![EnergyType::Water, EnergyType::Water]),
            PlayedCard::from_id(CardId::A4b122Wishiwashi),
            PlayedCard::from_id(CardId::A4b122Wishiwashi),
        ],
        vec![PlayedCard::from_id(CardId::A1001Bulbasaur)],
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
    // Base 30 + (2 * 40) = 110 damage. Bulbasaur has 70 HP, so it's KO'd.
    assert!(state.in_play_pokemon[1][0].is_none(), "Bulbasaur should be KO'd and removed");
}
