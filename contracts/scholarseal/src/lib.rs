#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, contracterror,
    token, Address, Env, String, Symbol,
};

#[contracterror]
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Error {
    NotInitialized         = 1,
    AlreadyInitialized     = 2,
    Unauthorized           = 3,
    GrantNotFound          = 4,
    GrantAlreadyExists     = 5,
    GrantAlreadyClaimed    = 6,
    InsufficientEscrow     = 7,
    GwaNotMet              = 8,
    EnrollmentHashMismatch = 9,
    WrongRecipient         = 10,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Grant(String),
    Admin,
    TokenId,
    EscrowBalance,
}

#[contracttype]
#[derive(Clone)]
pub struct GrantRecord {
    /// Unique student identifier e.g. "STU-2024-00142"
    pub student_id: String,
    /// Stellar wallet address that will receive USDC
    pub student_wallet: Address,
    /// USDC amount in stroops (10_000_000 = 1 USDC)
    pub amount: i128,
    /// SHA-256 hex string of the student's enrollment PDF
    pub enrollment_hash: String,
    /// Semester label e.g. "2024-2S"
    pub semester: String,
    /// True once the student has claimed and received USDC
    pub claimed: bool,
    /// Minimum GWA x100 required (0 = no requirement). Philippine scale: lower = better.
    pub min_gwa: u32,
    /// Student's actual GWA x100 e.g. 175 means 1.75
    pub student_gwa: u32,
}

#[contract]
pub struct ScholarSealContract;

#[contractimpl]
impl ScholarSealContract {

    pub fn initialize(env: Env, admin: Address, token_id: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::TokenId, &token_id);
        env.storage().instance().set(&DataKey::EscrowBalance, &0_i128);
        Ok(())
    }

    pub fn fund_escrow(env: Env, admin: Address, amount: i128) -> Result<(), Error> {
        let stored_admin: Address = env
            .storage().instance().get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;

        admin.require_auth();
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }

        let token_id: Address = env
            .storage().instance().get(&DataKey::TokenId)
            .ok_or(Error::NotInitialized)?;

        let token_client = token::Client::new(&env, &token_id);
        token_client.transfer(&admin, &env.current_contract_address(), &amount);

        let current: i128 = env
            .storage().instance().get(&DataKey::EscrowBalance)
            .unwrap_or(0);
        env.storage().instance().set(&DataKey::EscrowBalance, &(current + amount));
        Ok(())
    }

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
    ) -> Result<(), Error> {
        let stored_admin: Address = env
            .storage().instance().get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;

        admin.require_auth();
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }

        let key = DataKey::Grant(student_id.clone());
        if env.storage().persistent().has(&key) {
            return Err(Error::GrantAlreadyExists);
        }

        let escrow: i128 = env
            .storage().instance().get(&DataKey::EscrowBalance)
            .unwrap_or(0);
        if escrow < amount {
            return Err(Error::InsufficientEscrow);
        }

        // Philippine grading: lower number = better. 1.75 beats 2.00.
        // student_gwa must be <= min_gwa to qualify.
        if min_gwa > 0 && student_gwa > min_gwa {
            return Err(Error::GwaNotMet);
        }

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

        env.events().publish(
            (Symbol::new(&env, "grant_created"), student_id),
            amount,
        );
        Ok(())
    }

    pub fn claim_grant(
        env: Env,
        student_wallet: Address,
        student_id: String,
        enrollment_hash: String,
    ) -> Result<(), Error> {
        student_wallet.require_auth();

        let key = DataKey::Grant(student_id.clone());

        // Check existence BEFORE get — calling .get() on a missing key
        // causes UnreachableCodeReached in Soroban Wasm
        if !env.storage().persistent().has(&key) {
            return Err(Error::GrantNotFound);
        }

        let mut record: GrantRecord = env
            .storage().persistent().get(&key)
            .ok_or(Error::GrantNotFound)?;

        if record.claimed {
            return Err(Error::GrantAlreadyClaimed);
        }

        if student_wallet != record.student_wallet {
            return Err(Error::WrongRecipient);
        }

        if enrollment_hash != record.enrollment_hash {
            return Err(Error::EnrollmentHashMismatch);
        }

        // Mark claimed before transfer (checks-effects-interactions)
        record.claimed = true;
        env.storage().persistent().set(&key, &record);

        let escrow: i128 = env
            .storage().instance().get(&DataKey::EscrowBalance)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::EscrowBalance, &(escrow - record.amount));

        let token_id: Address = env
            .storage().instance().get(&DataKey::TokenId)
            .ok_or(Error::NotInitialized)?;

        let token_client = token::Client::new(&env, &token_id);
        token_client.transfer(
            &env.current_contract_address(),
            &record.student_wallet,
            &record.amount,
        );

        env.events().publish(
            (Symbol::new(&env, "grant_claimed"), student_id),
            record.amount,
        );
        Ok(())
    }

    /// Returns the grant record, or Error::GrantNotFound if student_id doesn't exist.
    /// Student IDs use format: STU-YYYY-NNNNN  e.g. STU-2024-00142
    pub fn get_grant(env: Env, student_id: String) -> Result<GrantRecord, Error> {
        let key = DataKey::Grant(student_id);
        if !env.storage().persistent().has(&key) {
            return Err(Error::GrantNotFound);
        }
        env.storage().persistent().get(&key).ok_or(Error::GrantNotFound)
    }

    pub fn get_escrow_balance(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::EscrowBalance)
            .unwrap_or(0)
    }

    pub fn get_admin(env: Env) -> Result<Address, Error> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)
    }
}

mod test;