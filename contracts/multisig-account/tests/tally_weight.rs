//! `tally_weight` counts a delegate listed twice only once (issue #1028).
//!
//! `__check_auth` admits a call when `weights::tally_weight` over the attached
//! delegates reaches the account's threshold. If a duplicated delegate were
//! summed once per occurrence, a single light signer could repeat itself up
//! to a heavier quorum. These tests pin that `tally_weight` credits each
//! registered signer at most once however often it is listed, and that a
//! duplicated signer therefore stays below a threshold it cannot reach alone.
//!
//! The Soroban host already refuses an authorization entry whose delegated
//! signers repeat an address ("delegated signers contain duplicate address"),
//! so today `__check_auth` never sees a duplicate; these tests pin the tally
//! itself so that guarantee does not rest on the host alone.

use multisig_account::{weights::tally_weight, Error, MultisigAccount, MultisigAccountClient};
use soroban_sdk::{testutils::Address as _, vec, Address, Env, Vec};

const WEIGHT_A: u32 = 10;
const WEIGHT_B: u32 = 20;

/// A two-signer account with `A = 10`, `B = 20` and threshold `20`: `B`
/// alone (or `A + B`) reaches it, `A` alone does not — but `A` counted twice
/// would.
struct Fx {
    env: Env,
    account: Address,
    a: Address,
    b: Address,
}

fn setup() -> Fx {
    let env = Env::default();
    env.mock_all_auths();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let account = env.register(MultisigAccount, (vec![&env, a.clone(), b.clone()], 2u32));
    let client = MultisigAccountClient::new(&env, &account);

    client.set_signer_weight(&a, &WEIGHT_A);
    client.set_signer_weight(&b, &WEIGHT_B);
    client.rotate_signers_and_threshold(&Vec::new(&env), &Vec::new(&env), &WEIGHT_B);
    assert_eq!(client.get_total_weight(), WEIGHT_A + WEIGHT_B);
    assert_eq!(client.get_threshold(), WEIGHT_B);

    Fx { env, account, a, b }
}

impl Fx {
    /// `tally_weight` over `delegates`, evaluated against the account's storage.
    fn tally(&self, delegates: &[&Address]) -> Result<u32, Error> {
        let mut list = Vec::new(&self.env);
        for d in delegates {
            list.push_back((*d).clone());
        }
        self.env
            .as_contract(&self.account, || tally_weight(&self.env, &list))
    }

    fn threshold(&self) -> u32 {
        MultisigAccountClient::new(&self.env, &self.account).get_threshold()
    }
}

#[test]
fn a_delegate_listed_twice_is_counted_once() {
    let fx = setup();
    let (a, b) = (&fx.a, &fx.b);

    // The issue's example: [A, A, B] is 10 + 20 = 30, not 40.
    assert_eq!(fx.tally(&[a, a, b]), Ok(WEIGHT_A + WEIGHT_B));
    // Same set, any order or multiplicity: still 30.
    assert_eq!(fx.tally(&[a, b]), Ok(30));
    assert_eq!(fx.tally(&[b, a, b, a]), Ok(30));
    assert_eq!(fx.tally(&[b, b, b, a]), Ok(30));
}

#[test]
fn a_single_delegate_repeated_contributes_its_weight_once() {
    let fx = setup();
    let (a, b) = (&fx.a, &fx.b);

    assert_eq!(fx.tally(&[a]), Ok(WEIGHT_A));
    assert_eq!(fx.tally(&[a, a]), Ok(WEIGHT_A));
    assert_eq!(fx.tally(&[a, a, a, a, a]), Ok(WEIGHT_A));
    assert_eq!(fx.tally(&[b, b]), Ok(WEIGHT_B));
}

#[test]
fn duplicating_a_light_signer_cannot_reach_the_threshold() {
    let fx = setup();
    let a = &fx.a;
    let threshold = fx.threshold();

    // A alone is below the threshold; listing A again must not change that,
    // even though 10 + 10 would exactly meet it.
    assert!(WEIGHT_A < threshold && WEIGHT_A * 2 >= threshold);
    for copies in [1usize, 2, 3, 8] {
        let delegates: std::vec::Vec<&Address> = core::iter::repeat_n(a, copies).collect();
        let weight = fx.tally(&delegates).unwrap();
        assert_eq!(weight, WEIGHT_A, "{copies} copies of A");
        assert!(
            weight < threshold,
            "{copies} copies of A must stay below threshold {threshold}"
        );
    }

    // A genuine second signer does reach it.
    assert!(fx.tally(&[a, &fx.b]).unwrap() >= threshold);
}

#[test]
fn an_unknown_delegate_is_still_rejected_alongside_duplicates() {
    let fx = setup();
    let outsider = Address::generate(&fx.env);
    let a = &fx.a;

    assert_eq!(fx.tally(&[a, a, &outsider]), Err(Error::UnknownSigner));
    assert_eq!(fx.tally(&[&outsider, &outsider]), Err(Error::UnknownSigner));
}
