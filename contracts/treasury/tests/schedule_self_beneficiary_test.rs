//! Self-beneficiary guard on `add_schedule` (issue #1019).
//!
//! A schedule naming the treasury itself as `beneficiary` could never pay
//! out: `claim_vested` would transfer from the contract to its own address,
//! book the amount as claimed, and leave the allocation stranded with no
//! revoke path. `add_schedule` (and `add_team_schedule`, which delegates to
//! it) must refuse such a schedule with [`Error::InvalidSchedule`] before
//! anything is written, while an external beneficiary keeps working.

#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events as _, Ledger},
    token::StellarAssetClient,
    Address, Env,
};
use treasury::{Error, Treasury, TreasuryClient};

const START: u64 = 1_700_000_000;
const CLIFF: u64 = 1_000;
const DURATION: u64 = 10_000;
const TOTAL: i128 = 1_000_000;

struct Ctx {
    env: Env,
    client: TreasuryClient<'static>,
    token: Address,
    treasury: Address,
}

fn setup() -> Ctx {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| li.timestamp = START);

    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let treasury = env.register(Treasury, (admin, token.clone()));
    let client = TreasuryClient::new(&env, &treasury);

    Ctx {
        env,
        client,
        token,
        treasury,
    }
}

#[test]
fn add_schedule_rejects_the_treasury_as_its_own_beneficiary() {
    let ctx = setup();

    assert_eq!(
        ctx.client
            .try_add_schedule(&ctx.treasury, &TOTAL, &START, &CLIFF, &DURATION),
        Err(Ok(Error::InvalidSchedule))
    );

    // Nothing was written and nothing was announced.
    assert_eq!(ctx.client.get_schedule(&ctx.treasury), None);
    let events = ctx.env.events().all().filter_by_contract(&ctx.treasury);
    assert_eq!(events.events().len(), 0);

    // With no schedule stored, a claim for the treasury has nothing to pay.
    assert_eq!(
        ctx.client.try_claim_vested(&ctx.treasury),
        Err(Ok(Error::ScheduleNotFound))
    );
}

#[test]
fn add_team_schedule_rejects_the_treasury_as_its_own_beneficiary() {
    let ctx = setup();

    assert_eq!(
        ctx.client
            .try_add_team_schedule(&ctx.treasury, &TOTAL, &START),
        Err(Ok(Error::InvalidSchedule))
    );
    assert_eq!(ctx.client.get_schedule(&ctx.treasury), None);
}

#[test]
fn add_schedule_still_accepts_an_external_beneficiary() {
    let ctx = setup();
    let beneficiary = Address::generate(&ctx.env);

    ctx.client
        .add_schedule(&beneficiary, &TOTAL, &START, &CLIFF, &DURATION);

    let schedule = ctx
        .client
        .get_schedule(&beneficiary)
        .expect("schedule for an external beneficiary is stored");
    assert_eq!(schedule.beneficiary, beneficiary);
    assert_eq!(schedule.total, TOTAL);
    assert_eq!(schedule.claimed, 0);

    // The allocation is actually reachable: once fully vested it pays out.
    StellarAssetClient::new(&ctx.env, &ctx.token).mint(&ctx.treasury, &TOTAL);
    ctx.env
        .ledger()
        .with_mut(|li| li.timestamp = START + DURATION);
    assert_eq!(ctx.client.claim_vested(&beneficiary), TOTAL);
}
