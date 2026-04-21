use crate::{
    actions::SimpleAction,
    effects::CardEffect,
    hooks::{contains_energy, get_attack_cost},
    State,
};

pub(crate) fn generate_attack_actions(state: &State) -> Vec<SimpleAction> {
    let current_player = state.current_player;
    let mut actions = Vec::new();
    if let Some(active_pokemon) = &state.in_play_pokemon[current_player][0] {
        // Fossil cards cannot attack
        if active_pokemon.is_fossil() {
            return actions;
        }

        // Check if the active Pokémon has the CannotAttack effect
        let active_effects = active_pokemon.get_active_effects();
        let cannot_attack = active_effects
            .iter()
            .any(|effect| matches!(effect, CardEffect::CannotAttack));
        if cannot_attack {
            return actions;
        }

        let restricted_attack_names: Vec<String> = active_effects
            .iter()
            .filter_map(|effect| match effect {
                CardEffect::CannotUseAttack(attack_name) => Some(attack_name.clone()),
                _ => None,
            })
            .collect();

        for (i, attack) in active_pokemon.get_attacks().iter().enumerate() {
            let modified_cost = get_attack_cost(&attack.energy_required, state, current_player);
            if contains_energy(active_pokemon, &modified_cost, state, current_player) {
                let attack_is_restricted = restricted_attack_names
                    .iter()
                    .any(|name| name == &attack.title);
                if attack_is_restricted {
                    continue;
                }

                // Check for bench Pokémon requirements (e.g. Mesprit's Supreme Blast)
                if let Some(effect_text) = &attack.effect {
                    if effect_text.contains("only if you have") && effect_text.contains("on your Bench") {
                        // Parse required Pokémon names from effect text like
                        // "You can use this attack only if you have Uxie and Azelf on your Bench."
                        let bench_names: Vec<String> = state
                            .enumerate_bench_pokemon(current_player)
                            .map(|(_, p)| p.get_name())
                            .collect();
                        // Extract names between "have" and "on your Bench"
                        if let Some(start) = effect_text.find("have ") {
                            if let Some(end) = effect_text.find(" on your Bench") {
                                let names_str = &effect_text[start + 5..end];
                                let required: Vec<&str> = names_str
                                    .split(" and ")
                                    .flat_map(|s| s.split(", "))
                                    .map(|s| s.trim())
                                    .collect();
                                let all_present = required
                                    .iter()
                                    .all(|name| bench_names.iter().any(|bn| bn == name));
                                if !all_present {
                                    continue;
                                }
                            }
                        }
                    }
                }

                actions.push(SimpleAction::Attack(i));
            }
        }
    }
    actions
}
