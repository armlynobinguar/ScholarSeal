# ScholarSeal 🎓

> On-chain scholarship disbursement for students who can't afford to wait.

---

## Problem

A 19-year-old nursing student in Cebu City qualifies for a government scholarship but waits 3–5 months for manual disbursement through a barangay official, losing her semester slot because tuition deadlines aren't extended — and she has no receipt, no proof of grant status, and no recourse if funds are misrouted.

## Project Description (Solution)

ScholarSeal lets school admins issue scholarship grants as verifiable on-chain records via Soroban, then automatically release USDC to enrolled students' wallets upon enrollment verification — using Stellar's near-zero fees and 5-second finality to replace slow, opaque manual cash disbursement.

---

## Stellar Features Used

| Feature | Purpose |
|--------|---------|
| **USDC transfers** | Stablecoin disbursement to student wallets |
| **Soroban smart contracts** | Conditional release, enrollment verification, escrow logic |
| **Trustlines** | Students must accept USDC before receiving funds |
| **Events** | Real-time dashboard updates on grant creation and claim |

---

## Timeline

| Phase | Description |
|-------|------------|
| Day 1 | Contract design, struct definitions, initialize + fund_escrow |
| Day 2 | create_grant + claim_grant logic, GWA check, hash verification |
| Day 3 | Test suite, deploy to testnet |
| Day 4 | Frontend dashboard (admin + student views) |
| Day 5 | Demo polish, README, pitch prep |

---

## Vision and Purpose

ScholarSeal targets a broken financial coordination problem that affects millions of students across Southeast Asia. Scholarship funds are frequently delayed, misrouted, or require students to physically collect cash from school cashiers or barangay officials.

By putting the disbursement logic on Soroban:
- **Admins** get a tamper-proof audit trail
- **Students** get instant, wallet-native USDC with no middleman
- **Sponsors** (NGOs, LGUs) get transparent fund tracking
- **Schools** reduce administrative overhead and fraud risk

---

## Prerequisites

- **Rust** `>=1.74.0` — [Install](https://rustup.rs/)
- **Soroban CLI** `>=20.0.0` — `cargo install --locked soroban-cli`
- **Stellar Testnet account** — [Friendbot](https://friendbot.stellar.org/)
- **USDC on testnet** — Use the Stellar testnet USDC issuer

---

## Build

```bash
# Build optimized Wasm for deployment
soroban contract build
```

Output: `target/wasm32-unknown-unknown/release/scholar_seal.wasm`

---

## Test

```bash
# Run all 5 tests
cargo test

# Run with output for debugging
cargo test -- --nocapture
```

---

## Deploy to Testnet

```bash
# 1. Set up testnet identity
soroban keys generate --global alice --network testnet

# 2. Fund with Friendbot
soroban keys fund alice --network testnet

# 3. Deploy contract
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/scholar_seal.wasm \
  --source alice \
  --network testnet

# Returns: CONTRACT_ID (save this)
```

---

## CLI Usage Examples

### Initialize the contract
```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source alice \
  --network testnet \
  -- initialize \
  --admin GADMIN_ADDRESS \
  --token_id GUSDC_TOKEN_ADDRESS
```

### Fund the escrow (admin deposits 500 USDC)
```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source alice \
  --network testnet \
  -- fund_escrow \
  --admin GADMIN_ADDRESS \
  --amount 5000000000
```

### Create a grant for a student
```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source alice \
  --network testnet \
  -- create_grant \
  --admin GADMIN_ADDRESS \
  --student_id '"STU-2024-00142"' \
  --student_wallet GSTUDENT_ADDRESS \
  --amount 5000000000 \
  --enrollment_hash '"a3f5c9e1b2d4f6a8c0e2b4d6f8a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a6"' \
  --semester '"2024-2S"' \
  --min_gwa 200 \
  --student_gwa 175
```

### Student claims their grant
```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --source student_key \
  --network testnet \
  -- claim_grant \
  --student_wallet GSTUDENT_ADDRESS \
  --student_id '"STU-2024-00142"' \
  --enrollment_hash '"a3f5c9e1b2d4f6a8c0e2b4d6f8a0c2e4b6d8f0a2c4e6b8d0f2a4c6e8b0d2f4a6"'
```

### View grant status
```bash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- get_grant \
  --student_id '"STU-2024-00142"'
```

---

## Project Structure

```
scholar_seal/
├── Cargo.toml
├── README.md
├── PROJECT_IDEA.md
└── src/
    ├── lib.rs      # Main Soroban contract
    └── test.rs     # 5-test suite
```

---

## Deployed Contract Details
[1] https://stellar.expert/explorer/testnet/tx/284007b65cc758f86f1765324a2b0f65099b8f0fac742da1724b2f063aa24ea2

[2] https://lab.stellar.org/r/testnet/contract/CCTACY7O7DMVHIMBPMP4GHHXFHKAJGUI6GDIGJMIDFCO6ILAPHF7H4J3

## Future Scope 
The development is still in progress 

## License

MIT License — free to use, modify, and deploy for educational and social impact purposes.
