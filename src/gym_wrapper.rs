// src/gym_wrapper.rs  — v3 obs space: 1200 features (1180 used + 20 reserved)
//
// Layout:
//   [0..26]     Section 1: State scalars       (26)
//   [26..68]    Section 2: Energy zone         (42 = (10+10+1)×2)
//   [68..580]   Section 3: Pokemon slots       (8 × 64 = 512)
//   [580..780]  Section 4: Own hand contents   (10 × 20 = 200)
//   [780..1180] Section 5: Action features     (25 × 16 = 400)
//   [1180..1200] Reserved                      (20)
//
// All fields verified against: state.rs, played_card.rs, card.rs

use pyo3::prelude::*;
use pyo3::types::PyDict;
use numpy::PyArray1;  // FIX 3: removed IntoPyArray (unused, API changed)
use rand::{rngs::StdRng, SeedableRng, RngCore};
use std::collections::HashSet;

use crate::{
    Deck, State,
    actions::{Action, SimpleAction},
    generate_possible_actions,
    state::GameOutcome,
    hooks::energy_missing,
    models::{Card, EnergyType, PlayedCard, TrainerType},
};

#[pyfunction]
pub fn test_gym_module() -> String {
    "Gym module works! (v3 obs: 1200 features)".to_string()
}

// =============================================================================
// Layout constants
// Keep in sync with deck_builder.py after maturin develop.
// =============================================================================

const N_ENERGY_TYPES:     usize = 10;

const STATE_SCALARS:      usize = 26;
const ENERGY_ZONE_FEAT:   usize = 42;   // (10+10+1) * 2
const POKEMON_SLOTS:      usize = 8;
const POKEMON_FEAT:       usize = 64;
const HAND_SLOTS:         usize = 10;
const HAND_FEAT:          usize = 20;
const ACTION_SLOTS:       usize = 25;
const ACTION_FEAT:        usize = 16;

const ENERGY_ZONE_OFFSET: usize = STATE_SCALARS;                                    // 26
const POKEMON_OFFSET:     usize = ENERGY_ZONE_OFFSET + ENERGY_ZONE_FEAT;            // 68
const HAND_OFFSET:        usize = POKEMON_OFFSET + POKEMON_SLOTS * POKEMON_FEAT;    // 580
const ACTION_OFFSET:      usize = HAND_OFFSET + HAND_SLOTS * HAND_FEAT;             // 780
pub const OBS_SIZE:       usize = 1200;

// =============================================================================
// Energy helpers
// =============================================================================

/// Canonical energy→index mapping. Matches deck_builder.py ENERGY_TYPE_IDX.
///   Grass=0  Fire=1  Water=2  Lightning=3  Psychic=4
///   Fighting=5  Darkness=6  Metal=7  Dragon=8  Colorless=9
#[inline]
fn energy_type_to_idx(e: EnergyType) -> usize {
    match e {
        EnergyType::Grass     => 0,
        EnergyType::Fire      => 1,
        EnergyType::Water     => 2,
        EnergyType::Lightning => 3,
        EnergyType::Psychic   => 4,
        EnergyType::Fighting  => 5,
        EnergyType::Darkness  => 6,
        EnergyType::Metal     => 7,
        EnergyType::Dragon    => 8,
        EnergyType::Colorless => 9,
    }
}

/// Most common non-Colorless energy type index in an attack's requirements.
/// Returns 9 (Colorless) if all requirements are Colorless or list is empty.
fn primary_energy_idx(energy_required: &[EnergyType]) -> usize {
    let mut counts = [0usize; N_ENERGY_TYPES];
    for e in energy_required {
        if *e != EnergyType::Colorless {
            counts[energy_type_to_idx(*e)] += 1;
        }
    }
    let (best_idx, best_count) = counts.iter().enumerate()
        .max_by_key(|(_, &c)| c)
        .unwrap_or((9, &0));
    if *best_count == 0 { 9 } else { best_idx }
}

// =============================================================================
// Pokemon slot encoder — 64 features
//
// [0]      is_present
// [1]      hp_fraction              remaining / total
// [2]      total_hp / 300
// [3]      ko_points / 3            1=normal 2=ex 3=mega
// [4]      stage / 2                0=basic 0.5=stage1 1.0=stage2
// [5]      is_ex
// [6]      has_ability
// [7]      ability_used
// [8]      played_this_turn
// [9]      has_tool
// [10–19]  weakness_type            10-hot; all zeros = no weakness
// [20]     attached_energy_total / 5
// [21–30]  attached_energy_by_type  10-dim; each = count_of_type/3 clamped
// [31]     retreat_cost / 4
// [32]     atk1_damage / 300
// [33]     atk1_energy_count / 4
// [34–43]  atk1_primary_energy_type 10-hot
// [44]     atk1_ready
// [45]     atk2_damage / 300
// [46]     atk2_energy_count / 4
// [47–56]  atk2_primary_energy_type 10-hot
// [57]     atk2_ready
// [58]     poisoned
// [59]     burned
// [60]     paralyzed
// [61]     asleep
// [62]     confused
// [63]     reserved = 0
// =============================================================================

fn encode_pokemon_slot(
    played: Option<&PlayedCard>,
    state:  &State,
    player: usize,
    out:    &mut [f32],
) {
    for x in out.iter_mut() { *x = 0.0; }
    let Some(pc) = played else { return; };

    // FIX 1: total_hp is a field on PokemonCard, not PlayedCard
    let total_hp = match &pc.card {
        Card::Pokemon(pcard) => pcard.hp.max(1) as f32,
        _ => 60.0f32,
    };
    let ko_pts   = pc.card.get_knockout_points() as f32;

    out[0] = 1.0;
    out[1] = pc.get_remaining_hp() as f32 / total_hp;  // FIX 1: method call
    out[2] = (total_hp / 300.0).min(1.0);
    out[3] = ko_pts / 3.0;

    if let Card::Pokemon(pcard) = &pc.card {
        out[4] = (pcard.stage as f32 / 2.0).min(1.0);
        out[5] = if pc.card.is_ex()            { 1.0 } else { 0.0 };
        out[6] = if pcard.ability.is_some()    { 1.0 } else { 0.0 };
        out[7] = if pc.ability_used            { 1.0 } else { 0.0 };
        out[8] = if pc.played_this_turn        { 1.0 } else { 0.0 };
        out[9] = if pc.attached_tool.is_some() { 1.0 } else { 0.0 };
        if let Some(weakness) = pcard.weakness {
            out[10 + energy_type_to_idx(weakness)] = 1.0;
        }
    }

    out[20] = (pc.attached_energy.len() as f32 / 5.0).min(1.0);
    for e in &pc.attached_energy {
        let idx = 21 + energy_type_to_idx(*e);
        out[idx] = (out[idx] + 1.0 / 3.0).min(1.0);
    }

    let retreat_len = pc.card.get_retreat_cost().map(|r| r.len()).unwrap_or(0) as f32;
    out[31] = (retreat_len / 4.0).min(1.0);

    let empty: Vec<crate::models::Attack> = vec![];
    let attacks: &Vec<crate::models::Attack> = match &pc.card {
        Card::Pokemon(_) => pc.get_attacks(),
        Card::Trainer(_) => &empty,
    };

    if let Some(a) = attacks.get(0) {
        out[32] = (a.fixed_damage as f32 / 300.0).min(1.0);
        out[33] = (a.energy_required.len() as f32 / 4.0).min(1.0);
        out[34 + primary_energy_idx(&a.energy_required)] = 1.0;
        let miss = energy_missing(pc, &a.energy_required, state, player).len();
        out[44] = if miss == 0 { 1.0 } else { 0.0 };
    }

    if let Some(a) = attacks.get(1) {
        out[45] = (a.fixed_damage as f32 / 300.0).min(1.0);
        out[46] = (a.energy_required.len() as f32 / 4.0).min(1.0);
        out[47 + primary_energy_idx(&a.energy_required)] = 1.0;
        let miss = energy_missing(pc, &a.energy_required, state, player).len();
        out[57] = if miss == 0 { 1.0 } else { 0.0 };
    }

    // FIX 2: status fields are private, use methods
    out[58] = if pc.is_poisoned()  { 1.0 } else { 0.0 };
    out[59] = if pc.is_burned()    { 1.0 } else { 0.0 };
    out[60] = if pc.is_paralyzed() { 1.0 } else { 0.0 };
    out[61] = if pc.is_asleep()    { 1.0 } else { 0.0 };
    out[62] = if pc.is_confused()  { 1.0 } else { 0.0 };
    // out[63] reserved = 0
}

// =============================================================================
// Hand slot encoder — 20 features
//
// [0]      is_present
// [1]      is_pokemon
// [2]      is_item        (TrainerType::Item or Fossil)
// [3]      is_supporter
// [4]      is_tool
// [5]      is_stadium
// [6]      is_playable_now
// [7]      card_hp / 300  (0 for trainers)
// [8]      card_is_ex     (0 for trainers)
// [9]      card_stage / 2 (0 for trainers)
// [10–19]  card_energy_type 10-hot (0 for trainers)
// =============================================================================

fn encode_hand_slot(card: Option<&Card>, is_playable: bool, out: &mut [f32]) {
    for x in out.iter_mut() { *x = 0.0; }
    let Some(card) = card else { return; };

    out[0] = 1.0;
    out[6] = if is_playable { 1.0 } else { 0.0 };

    match card {
        Card::Pokemon(pcard) => {
            out[1] = 1.0;
            out[7] = (pcard.hp as f32 / 300.0).min(1.0);
            out[8] = if card.is_ex() { 1.0 } else { 0.0 };
            out[9] = (pcard.stage as f32 / 2.0).min(1.0);
            out[10 + energy_type_to_idx(pcard.energy_type)] = 1.0;
        }
        Card::Trainer(tcard) => {
            match tcard.trainer_card_type {
                TrainerType::Supporter                  => out[3] = 1.0,
                TrainerType::Tool                       => out[4] = 1.0,
                TrainerType::Stadium                    => out[5] = 1.0,
                TrainerType::Item | TrainerType::Fossil => out[2] = 1.0,
            }
        }
    }
}

/// Card IDs playable from hand this turn (Place, Evolve-from-hand, Play actions).
fn playable_hand_card_ids(possible_actions: &[Action]) -> HashSet<String> {
    let mut ids = HashSet::new();
    for action in possible_actions {
        match &action.action {
            SimpleAction::Place(card, _) => { ids.insert(card.get_id()); }
            SimpleAction::Evolve { evolution, from_deck, .. } => {
                if !from_deck { ids.insert(evolution.get_id()); }
            }
            SimpleAction::Play { trainer_card, .. } => { ids.insert(Card::Trainer(trainer_card.clone()).get_id()); }
            _ => {}
        }
    }
    ids
}

/// Stable sort key: playable first, then Pokemon→Supporter→Item→Tool→Stadium→Fossil, then by ID.
fn hand_sort_key(card: &Card, is_playable: bool) -> (u8, u8, String) {
    let p = if is_playable { 0u8 } else { 1u8 };
    let t = match card {
        Card::Pokemon(_) => 0u8,
        Card::Trainer(tc) => match tc.trainer_card_type {
            TrainerType::Supporter => 1,
            TrainerType::Item      => 2,
            TrainerType::Tool      => 3,
            TrainerType::Stadium   => 4,
            TrainerType::Fossil    => 5,
        },
    };
    (p, t, card.get_id())
}

// =============================================================================
// Action slot encoder — 16 features
//
// [0]  is_end_turn
// [1]  is_attack
// [2]  is_attach_energy
// [3]  is_place_pokemon
// [4]  is_evolve
// [5]  is_retreat
// [6]  is_play_item       (non-Supporter trainers)
// [7]  is_play_supporter
// [8]  attack_damage / 300
// [9]  attack_energy_cost / 4
// [10] attack_is_ready
// [11] target_hp_fraction
// [12] target_ko_points / 3
// [13] would_ko
// [14] placed_card_hp / 300
// [15] placed_card_is_ex
// =============================================================================

fn encode_action_slot(
    action:       &SimpleAction,
    state:        &State,
    agent_player: usize,
    out:          &mut [f32],
) {
    for x in out.iter_mut() { *x = 0.0; }
    let opp = 1 - agent_player;

    match action {
        SimpleAction::EndTurn => { out[0] = 1.0; }

        SimpleAction::Attack(atk_idx) => {
            out[1] = 1.0;
            if let Some(active) = state.maybe_get_active(agent_player) {
                if let Some(atk) = active.get_attacks().get(*atk_idx) {
                    let dmg  = atk.fixed_damage as f32;
                    let miss = energy_missing(active, &atk.energy_required, state, agent_player).len();
                    out[8]  = (dmg / 300.0).min(1.0);
                    out[9]  = (atk.energy_required.len() as f32 / 4.0).min(1.0);
                    out[10] = if miss == 0 { 1.0 } else { 0.0 };
                    if let Some(opp_active) = state.maybe_get_active(opp) {
                        // FIX 1: use pcard.hp and get_remaining_hp()
                        let opp_total = match &opp_active.card {
                            Card::Pokemon(pcard) => pcard.hp.max(1) as f32,
                            _ => 60.0f32,
                        };
                        let opp_rem = opp_active.get_remaining_hp() as f32;
                        out[11] = opp_rem / opp_total;
                        out[12] = opp_active.card.get_knockout_points() as f32 / 3.0;
                        out[13] = if dmg >= opp_rem { 1.0 } else { 0.0 };
                    }
                }
            }
        }

        SimpleAction::Attach { .. } => { out[2] = 1.0; }

        SimpleAction::Place(card, _) => {
            out[3] = 1.0;
            if let Card::Pokemon(pc) = card {
                out[14] = (pc.hp as f32 / 300.0).min(1.0);
                out[15] = if card.is_ex() { 1.0 } else { 0.0 };
            }
        }

        SimpleAction::Evolve { evolution, .. } => {
            out[4] = 1.0;
            if let Card::Pokemon(pc) = evolution {
                out[14] = (pc.hp as f32 / 300.0).min(1.0);
                out[15] = if evolution.is_ex() { 1.0 } else { 0.0 };
            }
        }

        SimpleAction::Retreat(_) => { out[5] = 1.0; }

        SimpleAction::Play { trainer_card, .. } => {
            if trainer_card.trainer_card_type == TrainerType::Supporter { out[7] = 1.0; } else { out[6] = 1.0; }
        }

        // DrawCard, UseAbility, MoveEnergy, Activate, etc.: all bits 0.
        // The action mask still signals the action exists.
        _ => {}
    }
}

// =============================================================================
// Full observation builder
// =============================================================================

fn build_obs_for_player(
    state:            &State,
    viewer:           usize,
    possible_actions: &[Action],
) -> Vec<f32> {
    let opp = 1 - viewer;
    let mut obs = vec![0.0f32; OBS_SIZE];

    // ── Section 1: State scalars [0..26] ─────────────────────────────────
    let turn = state.turn_count as f32;
    obs[0]  = state.points[viewer] as f32 / 3.0;
    obs[1]  = state.points[opp] as f32 / 3.0;
    obs[2]  = (state.decks[viewer].cards.len() as f32 / 20.0).min(1.0);
    obs[3]  = (state.decks[opp].cards.len() as f32 / 20.0).min(1.0);
    obs[4]  = (turn / 100.0).min(1.0);
    obs[5]  = ((turn - 90.0).max(0.0) / 10.0).min(1.0);
    obs[6]  = (state.hands[viewer].len() as f32 / 10.0).min(1.0);
    obs[7]  = (state.hands[opp].len() as f32 / 10.0).min(1.0);
    obs[8]  = if state.has_played_support { 1.0 } else { 0.0 };
    obs[9]  = if state.current_energy.is_none() { 1.0 } else { 0.0 };
    obs[10] = if state.has_retreated { 1.0 } else { 0.0 };

    let own_nrg: usize = state.enumerate_in_play_pokemon(viewer).map(|(_, pc)| pc.attached_energy.len()).sum();
    let opp_nrg: usize = state.enumerate_in_play_pokemon(opp).map(|(_, pc)| pc.attached_energy.len()).sum();
    obs[11] = (own_nrg as f32 / 10.0).min(1.0);
    obs[12] = (opp_nrg as f32 / 10.0).min(1.0);
    obs[13] = state.enumerate_bench_pokemon(viewer).count() as f32 / 3.0;
    obs[14] = state.enumerate_bench_pokemon(opp).count() as f32 / 3.0;
    obs[15] = 0.0; // turn_timer — not tracked in engine

    let own_disc = &state.discard_piles[viewer];
    let opp_disc = &state.discard_piles[opp];
    obs[16] = (own_disc.len() as f32 / 40.0).min(1.0);
    obs[17] = (opp_disc.len() as f32 / 40.0).min(1.0);
    obs[18] = (own_disc.iter().filter(|c| matches!(c, Card::Pokemon(_))).count() as f32 / 10.0).min(1.0);
    obs[19] = (opp_disc.iter().filter(|c| matches!(c, Card::Pokemon(_))).count() as f32 / 10.0).min(1.0);
    obs[20] = (own_disc.iter().filter(|c| c.is_support()).count() as f32 / 10.0).min(1.0);
    obs[21] = (opp_disc.iter().filter(|c| c.is_support()).count() as f32 / 10.0).min(1.0);
    obs[22] = (state.discard_energies[viewer].len() as f32 / 10.0).min(1.0);
    obs[23] = (state.discard_energies[opp].len() as f32 / 10.0).min(1.0);
    obs[24] = if state.knocked_out_by_opponent_attack_this_turn { 1.0 } else { 0.0 };
    obs[25] = if state.knocked_out_by_opponent_attack_last_turn { 1.0 } else { 0.0 };

    // ── Section 2: Energy zone [26..68] ──────────────────────────────────
    // Sub-layout:
    //   [26–35] own_current_energy     (10-hot)   — Some on agent's turn
    //   [36–45] own_deck_energy_types  (10-multi)
    //   [46]    own_deck_is_single     (bool)
    //   [47–56] opp_current_energy     (zeros — unknowable when agent acts)
    //   [57–66] opp_deck_energy_types  (10-multi)
    //   [67]    opp_deck_is_single     (bool)
    if let Some(e) = state.current_energy {
        obs[26 + energy_type_to_idx(e)] = 1.0;
    }
    for e in &state.decks[viewer].energy_types {
        obs[36 + energy_type_to_idx(*e)] = 1.0;
    }
    obs[46] = if state.decks[viewer].energy_types.len() == 1 { 1.0 } else { 0.0 };
    // [47–56] opp current = 0
    for e in &state.decks[opp].energy_types {
        obs[57 + energy_type_to_idx(*e)] = 1.0;
    }
    obs[67] = if state.decks[opp].energy_types.len() == 1 { 1.0 } else { 0.0 };

    // ── Section 3: Pokemon slots [68..580] ───────────────────────────────
    // Slot 0: my active
    {
        let s = POKEMON_OFFSET;
        encode_pokemon_slot(state.maybe_get_active(viewer), state, viewer, &mut obs[s..s + POKEMON_FEAT]);
    }
    // Slots 1–3: my bench
    for (i, (_, pc)) in state.enumerate_bench_pokemon(viewer).take(3).enumerate() {
        let s = POKEMON_OFFSET + (1 + i) * POKEMON_FEAT;
        encode_pokemon_slot(Some(pc), state, viewer, &mut obs[s..s + POKEMON_FEAT]);
    }
    // Slot 4: opp active
    {
        let s = POKEMON_OFFSET + 4 * POKEMON_FEAT;
        encode_pokemon_slot(state.maybe_get_active(opp), state, opp, &mut obs[s..s + POKEMON_FEAT]);
    }
    // Slots 5–7: opp bench
    for (i, (_, pc)) in state.enumerate_bench_pokemon(opp).take(3).enumerate() {
        let s = POKEMON_OFFSET + (5 + i) * POKEMON_FEAT;
        encode_pokemon_slot(Some(pc), state, opp, &mut obs[s..s + POKEMON_FEAT]);
    }

    // ── Section 4: Own hand contents [580..780] ───────────────────────────
    let playable_ids = playable_hand_card_ids(possible_actions);
    let mut sorted_hand: Vec<(&Card, bool)> = state.hands[viewer]
        .iter()
        .map(|c| (c, playable_ids.contains(&c.get_id())))
        .collect();
    sorted_hand.sort_by_key(|(c, p)| hand_sort_key(c, *p));

    for (slot_i, (card, is_playable)) in sorted_hand.iter().enumerate().take(HAND_SLOTS) {
        let s = HAND_OFFSET + slot_i * HAND_FEAT;
        encode_hand_slot(Some(card), *is_playable, &mut obs[s..s + HAND_FEAT]);
    }

    // ── Section 5: Action features [780..1180] ────────────────────────────
    for (i, action) in possible_actions.iter().take(ACTION_SLOTS).enumerate() {
        let s = ACTION_OFFSET + i * ACTION_FEAT;
        encode_action_slot(&action.action, state, viewer, &mut obs[s..s + ACTION_FEAT]);
    }

    obs
}

fn build_mask(possible_actions: &[Action], max_actions: usize) -> Vec<i8> {
    let mut mask = vec![0i8; max_actions];
    for i in 0..possible_actions.len().min(max_actions) { mask[i] = 1; }
    mask
}

fn calculate_reward(winner: Option<GameOutcome>, agent_player: usize) -> f32 {
    match winner {
        Some(GameOutcome::Win(p)) if p == agent_player =>  1.0,
        Some(GameOutcome::Win(_))                      => -1.0,
        _                                              => -0.3,
    }
}

// =============================================================================
// PocketGym
// =============================================================================

#[pyclass(unsendable)]
pub struct PocketGym {
    agent_deck:      Deck,
    opponent_deck:   Deck,
    max_actions:     usize,
    opponent_policy: Option<PyObject>,
    current_state:   Option<State>,
    rng:             StdRng,
    possible_actions: Vec<Action>,
    game_over:       bool,
    agent_player:    usize,
}

#[pymethods]
impl PocketGym {
    #[new]
    #[pyo3(signature = (agent_deck_path, opponent_deck_path=None, max_actions=25, opponent_policy=None))]
    fn new(
        agent_deck_path:    String,
        opponent_deck_path: Option<String>,
        max_actions:        usize,
        opponent_policy:    Option<PyObject>,
    ) -> PyResult<Self> {
        let agent_deck = Deck::from_file(&agent_deck_path)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e))?;
        let opponent_deck = match opponent_deck_path {
            Some(p) => Deck::from_file(&p)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e))?,
            None => agent_deck.clone(),
        };
        Ok(PocketGym {
            agent_deck, opponent_deck, max_actions, opponent_policy,
            current_state: None,
            rng: StdRng::from_entropy(),
            possible_actions: Vec::new(),
            game_over: false,
            agent_player: 0,
        })
    }

    #[pyo3(signature = (policy=None))]
    fn set_opponent_policy(&mut self, policy: Option<PyObject>) { self.opponent_policy = policy; }

    fn is_selfplay_mode(&self) -> bool { self.opponent_policy.is_some() }

    #[getter]
    fn observation_size(&self) -> usize { OBS_SIZE }

    #[pyo3(signature = (seed=None))]
    fn reset<'py>(&mut self, py: Python<'py>, seed: Option<u64>)
        -> PyResult<(Py<PyArray1<f32>>, Py<PyDict>)>
    {
        use crate::{Game, players::{Player, WeightedRandomPlayer}};
        if let Some(s) = seed { self.rng = StdRng::seed_from_u64(s); }
        let players: Vec<Box<dyn Player>> = vec![
            Box::new(WeightedRandomPlayer { deck: self.agent_deck.clone() }),
            Box::new(WeightedRandomPlayer { deck: self.opponent_deck.clone() }),
        ];
        let mut game = Game::new(players, self.rng.next_u64());
        self.agent_player = 0;
        loop {
            let state = game.get_state_clone();
            if state.is_game_over() { break; }
            let (actor, _) = generate_possible_actions(&state);
            if actor == self.agent_player { break; }
            game.play_tick();
        }
        let state = game.get_state_clone();
        let (actor, actions) = generate_possible_actions(&state);
        self.possible_actions = if actor == self.agent_player { actions } else { vec![] };
        self.game_over = state.is_game_over();
        self.current_state = Some(state);
        Ok((self.build_observation(py)?, self.build_info(py)?))
    }

    fn step<'py>(&mut self, py: Python<'py>, action_idx: usize)
        -> PyResult<(Py<PyArray1<f32>>, f32, bool, bool, Py<PyDict>)>
    {
        use crate::{Game, players::{Player, WeightedRandomPlayer}, actions::apply_action};
        let state = self.current_state.as_mut()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Call reset() first"))?;
        if self.game_over {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Game over; call reset()"));
        }
        if self.possible_actions.is_empty() {
            return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("No valid actions"));
        }
        let action_idx = action_idx.min(self.possible_actions.len() - 1);
        let action = self.possible_actions[action_idx].clone();
        apply_action(&mut StdRng::from_entropy(), state, &action);

        let players: Vec<Box<dyn Player>> = vec![
            Box::new(WeightedRandomPlayer { deck: self.agent_deck.clone() }),
            Box::new(WeightedRandomPlayer { deck: self.opponent_deck.clone() }),
        ];
        let mut game = Game::from_state(state.clone(), players, self.rng.next_u64());

        loop {
            let cur = game.get_state_clone();
            if cur.is_game_over() {
                let reward = calculate_reward(cur.winner, self.agent_player);
                self.game_over = true;
                self.possible_actions = vec![];
                self.current_state = Some(cur);
                return Ok((self.build_observation(py)?, reward, true, false, self.build_info(py)?));
            }
            let (actor, actions) = generate_possible_actions(&cur);
            if actor == self.agent_player {
                self.possible_actions = actions;
                self.current_state = Some(cur);
                return Ok((self.build_observation(py)?, 0.0, false, false, self.build_info(py)?));
            }
            if actions.is_empty() { game.play_tick(); continue; }

            if let Some(ref policy) = self.opponent_policy {
                let opp = 1 - self.agent_player;
                let opp_obs  = build_obs_for_player(&cur, opp, &actions);
                let opp_mask = build_mask(&actions, self.max_actions);
                let chosen: usize = {
                    let obs_py  = PyArray1::from_slice_bound(py, &opp_obs);
                    let mask_py = PyArray1::from_slice_bound(py, &opp_mask);
                    policy.call1(py, (obs_py, mask_py))
                        .ok().and_then(|r| r.extract::<usize>(py).ok()).unwrap_or(0)
                };
                let safe = chosen.min(actions.len() - 1);
                let mut next = cur.clone();
                apply_action(&mut StdRng::from_entropy(), &mut next, &actions[safe]);
                game = Game::from_state(next, vec![
                    Box::new(WeightedRandomPlayer { deck: self.agent_deck.clone() }),
                    Box::new(WeightedRandomPlayer { deck: self.opponent_deck.clone() }),
                ], self.rng.next_u64());
            } else {
                game.play_tick();
            }
        }
    }

    fn action_masks<'py>(&self, py: Python<'py>) -> Py<PyArray1<i8>> {
        PyArray1::from_slice_bound(py, &build_mask(&self.possible_actions, self.max_actions)).unbind()
    }

    fn build_observation<'py>(&self, py: Python<'py>) -> PyResult<Py<PyArray1<f32>>> {
        let state = self.current_state.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("No state"))?;
        // FIX 3: use PyArray1::from_slice_bound
        Ok(PyArray1::from_slice_bound(py, &build_obs_for_player(state, self.agent_player, &self.possible_actions)).unbind())
    }

    fn build_info<'py>(&self, py: Python<'py>) -> PyResult<Py<PyDict>> {
        let state = self.current_state.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("No state"))?;
        let d = PyDict::new_bound(py);
        d.set_item("turn_count",        state.turn_count)?;
        d.set_item("num_valid_actions", self.possible_actions.len())?;
        d.set_item("game_over",         state.is_game_over())?;
        d.set_item("agent_player",      self.agent_player)?;
        d.set_item("selfplay_mode",     self.opponent_policy.is_some())?;
        d.set_item("obs_size",          OBS_SIZE)?;
        Ok(d.unbind())
    }
}

#[pyfunction]
#[pyo3(signature = (agent_deck_path, opponent_deck_path=None, max_actions=25, opponent_policy=None))]
pub fn create_gym(
    agent_deck_path:    String,
    opponent_deck_path: Option<String>,
    max_actions:        usize,
    opponent_policy:    Option<PyObject>,
) -> PyResult<PocketGym> {
    PocketGym::new(agent_deck_path, opponent_deck_path, max_actions, opponent_policy)
}

// =============================================================================
// EMM agent — unchanged
// =============================================================================

use crate::players::{ExpectiMiniMaxPlayer, ValueFunction, value_functions};

#[pyclass(unsendable)]
pub struct EmmAgent { player: ExpectiMiniMaxPlayer }

#[pymethods]
impl EmmAgent {
    #[new]
    #[pyo3(signature = (deck_path, max_depth=2))]
    fn new(deck_path: String, max_depth: usize) -> PyResult<Self> {
        let deck = Deck::from_file(&deck_path)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyValueError, _>(e))?;
        let vf: ValueFunction = Box::new(value_functions::baseline_value_function);
        Ok(EmmAgent { player: ExpectiMiniMaxPlayer {
            deck, max_depth, write_debug_trees: false, value_function: vf,
        }})
    }

    fn get_action(&mut self, gym: &PocketGym) -> PyResult<usize> {
        use crate::players::Player;
        let state = gym.current_state.as_ref()
            .ok_or_else(|| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("No state"))?;
        let (_, actions) = generate_possible_actions(state);
        if actions.is_empty() { return Ok(0); }
        let chosen = self.player.decision_fn(&mut StdRng::from_entropy(), state, &actions);
        Ok(actions.iter().position(|a|
            format!("{:?}", a.action) == format!("{:?}", chosen.action)
        ).unwrap_or(0))
    }
}

#[pyfunction]
#[pyo3(signature = (deck_path, max_depth=2))]
pub fn create_emm_agent(deck_path: String, max_depth: usize) -> PyResult<EmmAgent> {
    EmmAgent::new(deck_path, max_depth)
}