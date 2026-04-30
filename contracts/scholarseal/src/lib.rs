#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short,
    token, Address, Env, String, Symbol,
};

// ─────────────────────────────────────────────
// Storage Key Types
// ─────────────────────────────────────────────

/// Identifies a stored grant by student ID string
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Grant(String),      // student_id → GrantRecord
    Admin,              // → Address (contract administrator)
    TokenId,            // → Address (USDC token contract)
    EscrowBalance,      // → i128 (total USDC deposited by admin)
}

// ─────────────────────────────────────────────
// Data Structures
// ─────────────────────────────────────────────

/// The full lifecycle record of a single scholarship grant
#[contracttype]
#[derive(Clone)]
pub struct GrantRecord {
    /// Unique identifier for the student (e.g. student ID number)
    pub student_id: String,
    /// Stellar wallet address that will receive USDC
    pub student_wallet: Address,
    /// USDC amount in stroops (7 decimal places: 10_000_000 = 1 USDC)
    pub amount: i128,
    /// Enrollment verification hash (SHA-256 of enrollment doc, hex string)
    pub enrollment_hash: String,
    /// Semester label this grant is valid for, e.g. "2024-2S"
    pub semester: String,
    /// Whether the student has already claimed this grant
    pub claimed: bool,
    /// Minimum GWA (grade-weighted average × 100) required; 0 = no requirement
    pub min_gwa: u32,
    /// Student's GWA × 100 as submitted by admin (e.g. 175 = 1.75 GWA)
    pub student_gwa: u32,
}

// ─────────────────────────────────────────────
// Contract
// ─────────────────────────────────────────────

#[contract]
pub struct ScholarSealContract;

#[contractimpl]
impl ScholarSealContract {

    // ─── INITIALIZATION ───────────────────────

    /// Initialize the contract with an admin address and the USDC token contract address.
    /// Must be called once before any other function.
    pub fn initialize(env: Env, admin: Address, token_id: Address) {
        // Prevent re-initialization
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TokenId, &token_id);
        env.storage().instance().set(&DataKey::EscrowBalance, &0_i128);
    }

    // ─── ADMIN: FUND ESCROW ───────────────────

    /// Admin deposits USDC into the contract's escrow pool.
    /// This must be called before creating any grants — the contract
    /// will not create grants it cannot pay.
    pub fn fund_escrow(env: Env, admin: Address, amount: i128) {
        // Verify caller is the registered admin
        let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        if admin != stored_admin {
            panic!("unauthorized: only admin can fund escrow");
        }

        // Transfer USDC from admin wallet to this contract
        let token_id: Address = env.storage().instance().get(&DataKey::TokenId).unwrap();
        let token_client = token::Client::new(&env, &token_id);
        token_client.transfer(&admin, &env.current_contract_address(), &amount);

        // Update internal escrow balance tracker
        let current: i128 = env.storage().instance().get(&DataKey::EscrowBalance).unwrap();
        env.storage().instance().set(&DataKey::EscrowBalance, &(current + amount));
    }

    // ─── ADMIN: CREATE GRANT ──────────────────

    /// Admin registers a scholarship grant for a specific student.
    /// The student_id must be unique — one grant per student per call.
    /// enrollment_hash is a hex string of the SHA-256 of their enrollment PDF.
    /// min_gwa and student_gwa use integer × 100 encoding (e.g. 1.75 GWA → 175).
    pub fn create_grant(
        env: Env,
        admin: Address,
        student_id: String,
        student_wallet: Address,
        amount: i128,
        enrollment_hash: String,
        semester: String,
        min_gwa: u32,
        student_gwa: u32,
    ) {
        // Only admin may create grants
        let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        if admin != stored_admin {
            panic!("unauthorized: only admin can create grants");
        }

        // Prevent duplicate grants for the same student_id
        let key = DataKey::Grant(student_id.clone());
        if env.storage().persistent().has(&key) {
            panic!("grant already exists for this student_id");
        }

        // Ensure escrow has enough funds to cover this grant
        let escrow: i128 = env.storage().instance().get(&DataKey::EscrowBalance).unwrap();
        if escrow < amount {
            panic!("insufficient escrow balance to fund this grant");
        }

        // Check GWA eligibility — if min_gwa > 0, student must meet or beat it
        // Lower GWA number = better grade in Philippine grading (1.0 is highest)
        // So student_gwa must be <= min_gwa (e.g. student 175 <= required 200 ✓)
        if min_gwa > 0 && student_gwa > min_gwa {
            panic!("student does not meet minimum GWA requirement");
        }

        // Store the grant record in persistent storage
        let record = GrantRecord {
            student_id: student_id.clone(),
            student_wallet,
            amount,
            enrollment_hash,
            semester,
            claimed: false,
            min_gwa,
            student_gwa,
        };
        env.storage().persistent().set(&key, &record);

        // Emit an event so the frontend can update in real-time
        env.events().publish(
            (Symbol::new(&env, "grant_created"), student_id),
            amount,
        );
    }

    // ─── STUDENT: CLAIM GRANT ─────────────────

    /// Student calls this to claim their scholarship disbursement.
    /// The contract verifies:
    ///   1. The grant exists for this student_id
    ///   2. The grant has not already been claimed
    ///   3. The caller IS the registered student wallet (auth check)
    ///   4. The submitted enrollment_hash matches the one stored by admin
    /// On success, USDC is transferred from contract escrow to student wallet.
    pub fn claim_grant(
        env: Env,
        student_wallet: Address,
        student_id: String,
        enrollment_hash: String,
    ) {
        // Require the student's wallet to sign this transaction
        student_wallet.require_auth();

        // Load the grant record
        let key = DataKey::Grant(student_id.clone());
        let mut record: GrantRecord = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic!("no grant found for this student_id"));

        // Prevent double-claiming
        if record.claimed {
            panic!("grant already claimed");
        }

        // Verify the caller is the designated recipient
        if student_wallet != record.student_wallet {
            panic!("unauthorized: caller is not the grant recipient");
        }

        // Verify enrollment hash matches what admin submitted
        if enrollment_hash != record.enrollment_hash {
            panic!("enrollment verification failed: hash mismatch");
        }

        // Mark as claimed before transferring (checks-effects-interactions pattern)
        record.claimed = true;
        env.storage().persistent().set(&key, &record);

        // Deduct from escrow balance tracker
        let escrow: i128 = env.storage().instance().get(&DataKey::EscrowBalance).unwrap();
        env.storage()
            .instance()
            .set(&DataKey::EscrowBalance, &(escrow - record.amount));

        // Transfer USDC from this contract to the student's wallet
        let token_id: Address = env.storage().instance().get(&DataKey::TokenId).unwrap();
        let token_client = token::Client::new(&env, &token_id);
        token_client.transfer(
            &env.current_contract_address(),
            &record.student_wallet,
            &record.amount,
        );

        // Emit disbursement event for dashboard listeners
        env.events().publish(
            (Symbol::new(&env, "grant_claimed"), student_id),
            record.amount,
        );
    }

    // ─── VIEW FUNCTIONS ───────────────────────

    /// Returns the full grant record for a given student_id.
    /// Useful for the frontend to show grant status without a transaction.
    pub fn get_grant(env: Env, student_id: String) -> GrantRecord {
        let key = DataKey::Grant(student_id);
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic!("no grant found"))
    }

    /// Returns the current USDC escrow balance held by this contract.
    pub fn get_escrow_balance(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::EscrowBalance)
            .unwrap_or(0)
    }

    /// Returns the registered admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }
}

mod test;