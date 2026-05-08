use deckgym::{
    actions::{Action, SimpleAction},
    card_ids::CardId,
    models::{EnergyType, PlayedCard},
    test_support::get_initialized_game,
};

#[test]
fn test_turtonator_shell_trap() {
    let mut game = get_initialized_game(0);
    let mut state = game.get_state_clone();

    state.set_board(
        vec![PlayedCard::from_id(CardId::B1047Turtonator)
            .with_energy(vec![EnergyType::Fire, EnergyType::Fire])],
        vec![PlayedCard::from_id(CardId::A1001Bulbasaur).with_energy(vec![EnergyType::Grass, EnergyType::Grass])],
    );
    state.current_player = 0;
    game.set_state(state);

    // Turtonator attacks
    let attack_action = Action {
        actor: 0,
        action: SimpleAction::Attack(0),
        is_stack: false,
    };
    game.apply_action(&attack_action);

    let state = game.get_state_clone();
    // Base damage 40 + 20 (Weakness) = 60 -> Bulbasaur has 10 HP left (70 - 60)
    assert_eq!(state.get_active(1).get_remaining_hp(), 10);
    
    // Now it's Bulbasaur's turn
    let end_turn_action = Action { actor: 0, action: SimpleAction::EndTurn, is_stack: false };
    game.apply_action(&end_turn_action);
    
    // Bulbasaur attacks Turtonator (attack 0: Vine Whip, 40 damage)
    let opp_attack = Action { actor: 1, action: SimpleAction::Attack(0), is_stack: false };
    game.apply_action(&opp_attack);
    
    // Turtonator takes 40 damage (110 - 40 = 70 HP)
    let state = game.get_state_clone();
    assert_eq!(state.get_active(0).get_remaining_hp(), 70, "Turtonator takes 40 damage");
    
    // Opponent should be choosing a promotion or game ended if no bench
    assert!(state.winner.is_some(), "Bulbasaur is KO'd by Revenge Damage and game ends");
}
