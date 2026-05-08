use deckgym::{
    actions::{Action, SimpleAction},
    card_ids::CardId,
    models::{EnergyType, PlayedCard},
    test_support::get_initialized_game,
};

#[test]
fn test_tapu_koko_attack_with_lightning_bench() {
    let mut game = get_initialized_game(0);
    let mut state = game.get_state_clone();

    state.set_board(
        vec![
            PlayedCard::from_id(CardId::A3166TapuKoko)
                .with_energy(vec![EnergyType::Lightning, EnergyType::Colorless]),
            PlayedCard::from_id(CardId::A1094Pikachu), // Lightning
        ],
        vec![PlayedCard::from_id(CardId::A1002Ivysaur)],
    );
    state.current_player = 0;
    game.set_state(state);

    let attack_action = Action {
        actor: 0,
        action: SimpleAction::Attack(0),
        is_stack: false,
    };
    game.apply_action(&attack_action);

    // Verify damage: 90 - 70 = 20
    let state = game.get_state_clone();
    assert_eq!(state.get_active(1).get_remaining_hp(), 20, "Base damage is 70");
    
    // Verify switch choices exist in the move generation stack
    assert_eq!(state.move_generation_stack.len(), 1);
    let (_, choices) = state.move_generation_stack.last().unwrap();
    assert_eq!(choices.len(), 1);
    if let SimpleAction::Activate { player, in_play_idx } = choices[0] {
        assert_eq!(player, 0);
        assert_eq!(in_play_idx, 1);
    } else {
        panic!("Expected Activate action");
    }
}
