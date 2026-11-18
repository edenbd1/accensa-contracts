//! Governance member cap (issue #1025).
//!
//! `MAX_MEMBERS` is 32: `__constructor` must seed exactly 32 members in full,
//! and a 33rd member must be refused with the typed [`Error::InvalidMembers`]
//! rather than being silently dropped or trapping without a contract error.

use governance::{Error, Governance, GovernanceClient};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

/// The cap these tests pin (`MAX_MEMBERS` in `contracts/governance/src/lib.rs`).
const CAP: u32 = 32;
const THRESHOLD_BPS: u32 = 5_000;
const VOTING_PERIOD: u32 = 10;

/// `count` distinct members. Member `i` deposits `(i + 1)^2`, so its quadratic
/// voting weight is exactly `i + 1` and every member's weight is distinct: a
/// dropped, duplicated or mispaired member would change at least one query.
fn members_and_deposits(env: &Env, count: u32) -> (Vec<Address>, Vec<u64>) {
    let mut members = Vec::new(env);
    let mut deposits = Vec::new(env);
    for i in 0..count {
        members.push_back(Address::generate(env));
        let root = u64::from(i) + 1;
        deposits.push_back(root * root);
    }
    (members, deposits)
}

#[test]
fn exactly_thirty_two_members_initialize_with_their_full_weights() {
    let env = Env::default();
    let (members, deposits) = members_and_deposits(&env, CAP);
    assert_eq!(members.len(), CAP);

    let id = env.register(
        Governance,
        (
            members.clone(),
            deposits.clone(),
            THRESHOLD_BPS,
            VOTING_PERIOD,
        ),
    );
    let gov = GovernanceClient::new(&env, &id);

    let mut expected_total_weight = 0u64;
    let mut expected_total_deposits = 0u64;
    for i in 0..CAP {
        let member = members.get(i).unwrap();
        let deposit = deposits.get(i).unwrap();
        let weight = u64::from(i) + 1;

        assert!(gov.is_member(&member), "member {i} must be registered");
        assert_eq!(gov.get_member_deposit(&member), deposit);
        assert_eq!(gov.get_member_weight(&member), weight);

        expected_total_weight += weight;
        expected_total_deposits += deposit;
    }
    // 1 + 2 + ... + 32 and 1^2 + 2^2 + ... + 32^2: nothing was truncated.
    assert_eq!(expected_total_weight, 528);
    assert_eq!(gov.get_total_weight(), expected_total_weight);
    assert_eq!(expected_total_deposits, 11_440);
    assert_eq!(gov.get_total_deposits(), expected_total_deposits);
}

/// The constructor body is called directly (inside a live instance, as in
/// `receipt_anchor_admin.rs`) so its `Result` — not just a trap — is
/// observable: 33 members is `Error::InvalidMembers`.
#[test]
fn a_thirty_third_member_is_rejected_with_invalid_members() {
    let env = Env::default();
    let (seed, seed_deposit) = members_and_deposits(&env, 1);
    let host = env.register(
        Governance,
        (seed, seed_deposit, THRESHOLD_BPS, VOTING_PERIOD),
    );

    let (members, deposits) = members_and_deposits(&env, CAP + 1);
    assert_eq!(members.len(), 33);

    let result = env.as_contract(&host, || {
        Governance::__constructor(
            env.clone(),
            members.clone(),
            deposits,
            THRESHOLD_BPS,
            VOTING_PERIOD,
        )
    });
    assert_eq!(result, Err(Error::InvalidMembers));

    // The rejected call wrote nothing: none of the 33 became a member.
    let gov = GovernanceClient::new(&env, &host);
    for member in members.iter() {
        assert!(!gov.is_member(&member));
    }
    assert_eq!(gov.get_total_weight(), 1);
}
