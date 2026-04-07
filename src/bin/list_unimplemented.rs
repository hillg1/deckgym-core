use deckgym::{
    card_ids::CardId,
    card_validation::{get_implementation_status, ImplementationStatus},
    database::get_card_by_enum,
};
use strum::IntoEnumIterator;

fn is_b2a_or_earlier(card_id: &CardId) -> bool {
    // Currently we just check if it parses, or we can look up the ID prefix.
    // For now we list all cards.
    true
}

fn main() {
    let mut missing_attacks = Vec::new();
    let mut missing_abilities = Vec::new();
    let mut missing_trainers = Vec::new();
    let mut missing_tools = Vec::new();

    for card_id in CardId::iter() {
        if !is_b2a_or_earlier(&card_id) {
            continue;
        }

        let status = get_implementation_status(card_id);
        if status != ImplementationStatus::Complete {
            let card = get_card_by_enum(card_id);
            let name = card.get_name();
            match status {
                ImplementationStatus::MissingAttack => missing_attacks.push((name, card_id)),
                ImplementationStatus::MissingAbility => missing_abilities.push((name, card_id)),
                ImplementationStatus::MissingTrainer => missing_trainers.push((name, card_id)),
                ImplementationStatus::MissingTool => missing_tools.push((name, card_id)),
                _ => {}
            }
        }
    }

    println!("=====================================");
    println!("MISSING IMPLEMENTATIONS - BY CATEGORY");
    println!("=====================================\n");

    println!("--- Missing Attacks ({}) ---", missing_attacks.len());
    for (name, id) in &missing_attacks {
        println!("- {name} ({id:?})");
    }
    
    println!("\n--- Missing Abilities ({}) ---", missing_abilities.len());
    for (name, id) in &missing_abilities {
        println!("- {name} ({id:?})");
    }

    println!("\n--- Missing Trainers ({}) ---", missing_trainers.len());
    for (name, id) in &missing_trainers {
        println!("- {name} ({id:?})");
    }
    
    println!("\n--- Missing Tools ({}) ---", missing_tools.len());
    for (name, id) in &missing_tools {
        println!("- {name} ({id:?})");
    }
}
