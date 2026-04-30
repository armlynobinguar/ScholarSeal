#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation},
    token, Address, Env, IntoVal, String,
};
use token::Client as TokenClient;
use token::StellarAssetClient;

// ─────────────────────────────────────────────
// Test Helpers
// ─────────────────────────────────────────────

/// Sets up a fresh environment with:
/// - A deployed ScholarSeal contract
/// - A mock USDC token (Stellar Asset Contract)
/// - An admin address with 10,000 USDC minted
/// Returns (env, contract_id, token_id, admin)
fn setup() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    // Deploy USDC mock using the Stellar Asset Contract
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract(token_admin.clone());
    let usdc = StellarAssetClient::new(&env, &token_id);

    // Generate admin and mint them 10,000 USDC (10_000 * 10^7 stroops)
    let admin = Address::generate(&env);
    usdc.mint(&admin, &100_000_000_000_i128); // 10,000 USDC

    // Deploy ScholarSeal contract
    let contract_id = env.register_contract(None, ScholarSealContract);
    let client = ScholarSealContractClient::new(&env, &contract_id);
    client.initialize(&admin, &token_id);

    (env, contract_id, token_id, admin)
}

/// Returns a standard student setup: (student_address, student_id, enrollment_hash, semester)
fn make_student(env: &Env) -> (Address, String, String, String) {
    let student = Address::generate(env);
    let student_id = String::from_str(env, "STU-2024-00142");
    let enrollment_hash = String::from_str(
        env,
        "a3f5c9e1b2d4f6a8c0e2b4d6f8a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a6",
    );
    let semester = String::from_str(env, "2024-2S");
    (student, student_id, enrollment_hash, semester)
}

// ─────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test 1: Happy Path ────────────────────
    // The full MVP flow executes end-to-end:
    // Admin funds escrow → creates grant → student claims → USDC received
    #[test]
    fn test_happy_path_full_disbursement() {
        let (env, contract_id, token_id, admin) = setup();
        let client = ScholarSealContractClient::new(&env, &contract_id);
        let token = TokenClient::new(&env, &token_id);
        let (student, student_id, enrollment_hash, semester) = make_student(&env);

        // Grant amount: 500 USDC = 5_000_000_000 stroops
        let grant_amount: i128 = 5_000_000_000;

        // Step 1: Admin funds the escrow
        client.fund_escrow(&admin, &grant_amount);
        assert_eq!(client.get_escrow_balance(), grant_amount);

        // Step 2: Admin creates the grant for the student
        client.create_grant(
            &admin,
            &student_id,
            &student,
            &grant_amount,
            &enrollment_hash,
            &semester,
            &200_u32, // min GWA 2.00
            &175_u32, // student GWA 1.75 (meets requirement)
        );

        // Verify grant was stored and not yet claimed
        let grant = client.get_grant(&student_id);
        assert!(!grant.claimed);
        assert_eq!(grant.amount, grant_amount);

        // Record student balance before claim
        let balance_before = token.balance(&student);

        // Step 3: Student claims their grant
        client.claim_grant(&student, &student_id, &enrollment_hash);

        // Step 4: Verify USDC arrived in student's wallet
        let balance_after = token.balance(&student);
        assert_eq!(balance_after - balance_before, grant_amount);

        // Escrow should now be zero
        assert_eq!(client.get_escrow_balance(), 0);
    }

    // ── Test 2: Edge Case — Double Claim Blocked ──
    // Once a student claims a grant, any repeat attempt must fail.
    // This prevents double-spending from the escrow.
    #[test]
    #[should_panic(expected = "grant already claimed")]
    fn test_cannot_claim_grant_twice() {
        let (env, contract_id, _token_id, admin) = setup();
        let client = ScholarSealContractClient::new(&env, &contract_id);
        let (student, student_id, enrollment_hash, semester) = make_student(&env);

        let grant_amount: i128 = 5_000_000_000;

        client.fund_escrow(&admin, &(grant_amount * 2)); // fund enough for 2 (should still fail)
        client.create_grant(
            &admin,
            &student_id,
            &student,
            &grant_amount,
            &enrollment_hash,
            &semester,
            &0_u32,
            &0_u32,
        );

        // First claim succeeds
        client.claim_grant(&student, &student_id, &enrollment_hash);

        // Second claim must panic
        client.claim_grant(&student, &student_id, &enrollment_hash);
    }

    // ── Test 3: State Verification ────────────
    // After a successful claim, the grant record in storage
    // must reflect claimed = true and the escrow balance must decrease.
    #[test]
    fn test_state_reflects_claimed_grant() {
        let (env, contract_id, _token_id, admin) = setup();
        let client = ScholarSealContractClient::new(&env, &contract_id);
        let (student, student_id, enrollment_hash, semester) = make_student(&env);

        let grant_amount: i128 = 3_000_000_000; // 300 USDC

        client.fund_escrow(&admin, &grant_amount);
        client.create_grant(
            &admin,
            &student_id,
            &student,
            &grant_amount,
            &enrollment_hash,
            &semester,
            &0_u32,
            &0_u32,
        );

        // State before claim
        let before = client.get_grant(&student_id);
        assert!(!before.claimed, "Grant should not be claimed yet");
        assert_eq!(client.get_escrow_balance(), grant_amount);

        client.claim_grant(&student, &student_id, &enrollment_hash);

        // State after claim
        let after = client.get_grant(&student_id);
        assert!(after.claimed, "Grant must be marked claimed after disbursement");
        assert_eq!(
            client.get_escrow_balance(),
            0,
            "Escrow must be empty after disbursement"
        );
    }

    // ── Test 4: GWA Eligibility Check ─────────
    // A student who does not meet the minimum GWA requirement
    // must be blocked from receiving the grant at creation time.
    #[test]
    #[should_panic(expected = "student does not meet minimum GWA requirement")]
    fn test_gwa_below_minimum_is_rejected() {
        let (env, contract_id, _token_id, admin) = setup();
        let client = ScholarSealContractClient::new(&env, &contract_id);
        let (student, student_id, enrollment_hash, semester) = make_student(&env);

        client.fund_escrow(&admin, &5_000_000_000);

        // min GWA 1.75 (175) but student has 2.25 GWA (225) — fails
        client.create_grant(
            &admin,
            &student_id,
            &student,
            &5_000_000_000,
            &enrollment_hash,
            &semester,
            &175_u32, // require 1.75 or better
            &225_u32, // student has 2.25 — does NOT qualify
        );
    }

    // ── Test 5: Enrollment Hash Mismatch Blocked ──
    // If a student submits the wrong enrollment hash at claim time,
    // the contract must reject the disbursement to prevent fraud.
    #[test]
    #[should_panic(expected = "enrollment verification failed: hash mismatch")]
    fn test_wrong_enrollment_hash_rejected() {
        let (env, contract_id, _token_id, admin) = setup();
        let client = ScholarSealContractClient::new(&env, &contract_id);
        let (student, student_id, enrollment_hash, semester) = make_student(&env);

        client.fund_escrow(&admin, &5_000_000_000);
        client.create_grant(
            &admin,
            &student_id,
            &student,
            &5_000_000_000,
            &enrollment_hash, // correct hash stored here
            &semester,
            &0_u32,
            &0_u32,
        );

        // Student submits a tampered / wrong hash
        let fake_hash = String::from_str(
            &env,
            "0000000000000000000000000000000000000000000000000000000000000000",
        );
        client.claim_grant(&student, &student_id, &fake_hash);
    }
}