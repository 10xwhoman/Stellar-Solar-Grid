#[cfg(test)]
mod additional_tests {
    use super::*;
    use soroban_sdk::{
        symbol_short,
        testutils::{Address as _, Events, Ledger},
        token, Address, Env, Symbol, TryFromVal,
    };

    fn setup() -> (Env, SolarGridContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SolarGridContract);
        let client = SolarGridContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        (env, client, admin)
    }

    fn setup_with_token() -> (Env, SolarGridContractClient<'static>, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, SolarGridContract);
        let client = SolarGridContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let token_address = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        client.initialize(&admin, &token_address);
        (env, client, admin, token_address)
    }

    #[test]
    fn test_register_and_payment() {
        let (env, client, admin) = setup();
        let meter_id = String::from_str(&env, "SG-2024-METER1");
        let owner = Address::generate(&env);
        
        // Initialize contract first
        let token_admin = Address::generate(&env);
        let token_address = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        client.initialize(&admin, &token_address);
        
        // Register meter
        client.allowlist_add(&owner);
        client.register_meter(&meter_id, &owner);
        
        // Verify meter exists and is inactive
        let meter = client.get_meter(&meter_id).unwrap();
        assert!(!meter.active);
        assert_eq!(meter.owner, owner);
    }

    #[test]
    fn test_double_register_fails() {
        let (env, client, admin) = setup();
        let meter_id = String::from_str(&env, "SG-2024-METER1");
        let owner = Address::generate(&env);
        
        // Initialize contract
        let token_admin = Address::generate(&env);
        let token_address = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        client.initialize(&admin, &token_address);
        
        // Register meter once
        client.allowlist_add(&owner);
        client.register_meter(&meter_id, &owner);
        
        // Try to register again - should fail
        let result = std::panic::catch_unwind(|| {
            client.register_meter(&meter_id, &owner);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_payment_on_inactive_meter() {
        let (env, client, admin, token_address) = setup_with_token();
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        let meter_id = String::from_str(&env, "SG-2024-METER1");
        let owner = Address::generate(&env);
        
        // Register meter but don't activate
        client.allowlist_add(&owner);
        client.register_meter(&meter_id, &owner);
        
        // Try to make payment - should work but meter should become active
        token_admin_client.mint(&owner, &1_000_000_i128);
        client.make_payment(&meter_id, &owner, &1_000_000_i128, &PaymentPlan::Daily);
        
        // Verify meter is now active
        assert!(client.check_access(&meter_id));
    }

    #[test]
    fn test_zero_balance_access() {
        let (env, client, admin, token_address) = setup_with_token();
        let meter_id = String::from_str(&env, "SG-2024-METER1");
        let owner = Address::generate(&env);
        
        // Register meter
        client.allowlist_add(&owner);
        client.register_meter(&meter_id, &owner);
        
        // Check access with zero balance - should be false
        assert!(!client.check_access(&meter_id));
    }

    #[test]
    fn test_usage_update_edge_cases() {
        let (env, client, admin, token_address) = setup_with_token();
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        let oracle = Address::generate(&env);
        client.set_oracle(&oracle);
        
        let meter_id = String::from_str(&env, "SG-2024-METER1");
        let owner = Address::generate(&env);
        
        // Setup meter with payment
        client.allowlist_add(&owner);
        client.register_meter(&meter_id, &owner);
        token_admin_client.mint(&owner, &5_000_000_i128);
        client.make_payment(&meter_id, &owner, &5_000_000_i128, &PaymentPlan::Daily);
        
        // Test large usage update
        env.mock_all_auths_allowing_non_root_auth();
        client.update_usage(&meter_id, &4_999_999_i128);
        
        // Verify balance is nearly depleted
        let meter = client.get_meter(&meter_id).unwrap();
        assert!(meter.balance < 10_i128);
    }

    // ---------------------------------------------------------------------
    // Installment payment plan tests
    // ---------------------------------------------------------------------

    const DAY: u64 = 86_400;
    const MONTH: u64 = 30 * DAY;

    fn allowlist_and_register(
        env: &Env,
        client: &SolarGridContractClient<'static>,
        owner: &Address,
    ) -> String {
        let meter_id = String::from_str(env, "SG-2024-METER1");
        client.allowlist_add(owner);
        client.register_meter(&meter_id, owner);
        meter_id
    }

    fn events_as_tuples(env: &Env) -> std::vec::Vec<(Symbol, soroban_sdk::Val)> {
        env.events()
            .all()
            .iter()
            .map(|(_, topics, _)| {
                let sym = Symbol::try_from_val(env, &topics.get(0).unwrap()).unwrap();
                (sym, topics.get(1).unwrap())
            })
            .collect()
    }

    #[test]
    fn test_create_installment_plan_happy_path() {
        let (env, client, _admin, token_address) = setup_with_token();
        let owner = Address::generate(&env);
        let _meter_id = allowlist_and_register(&env, &client, &owner);

        let total: i128 = 3_000_000;
        let installments: u32 = 3;
        let now = env.ledger().timestamp();

        let plan_id = client.create_installment_plan(&owner, &total, &installments);
        assert_eq!(plan_id, 0);

        let plan = client.get_installment_plan(&plan_id).unwrap();
        assert_eq!(plan.payer, owner);
        assert_eq!(plan.total, total);
        assert_eq!(plan.installments, installments);
        assert_eq!(plan.installment_amount, total / installments as i128);
        assert_eq!(plan.paid, 0);
        assert_eq!(plan.remaining, total);
        assert_eq!(plan.next_due, now + MONTH);
        assert_eq!(plan.status, InstallmentStatus::Active);

        let events = events_as_tuples(&env);
        assert!(events.iter().any(|(sym, _)| *sym == symbol_short!("inst_new")));

        let _ = token_address;
    }

    #[test]
    fn test_create_installment_plan_invalid_total() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let owner = Address::generate(&env);
        let _meter_id = allowlist_and_register(&env, &client, &owner);

        let result = client.try_create_installment_plan(&owner, &0_i128, &3_u32);
        assert_eq!(result, Err(Ok(Error::InvalidInstallmentPlan)));

        let result = client.try_create_installment_plan(&owner, &-1_i128, &3_u32);
        assert_eq!(result, Err(Ok(Error::InvalidInstallmentPlan)));
    }

    #[test]
    fn test_create_installment_plan_invalid_installments() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let owner = Address::generate(&env);
        let _meter_id = allowlist_and_register(&env, &client, &owner);

        let result = client.try_create_installment_plan(&owner, &1_000_000_i128, &0_u32);
        assert_eq!(result, Err(Ok(Error::InvalidInstallmentPlan)));

        let result = client.try_create_installment_plan(&owner, &1_000_000_i128, &61_u32);
        assert_eq!(result, Err(Ok(Error::InvalidInstallmentPlan)));
    }

    #[test]
    fn test_pay_installment_not_due() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let owner = Address::generate(&env);
        let _meter_id = allowlist_and_register(&env, &client, &owner);

        let plan_id = client.create_installment_plan(&owner, &3_000_000_i128, &3_u32);

        let result = client.try_pay_installment(&plan_id, &owner);
        assert_eq!(result, Err(Ok(Error::InstallmentNotDue)));
    }

    #[test]
    fn test_pay_installment_due() {
        let (env, client, _admin, token_address) = setup_with_token();
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        let owner = Address::generate(&env);
        let _meter_id = allowlist_and_register(&env, &client, &owner);

        let total: i128 = 3_000_000;
        let installments: u32 = 3;
        let plan_id = client.create_installment_plan(&owner, &total, &installments);

        token_admin_client.mint(&owner, &total);

        let plan_before = client.get_installment_plan(&plan_id).unwrap();
        env.ledger().set_timestamp(plan_before.next_due);

        client.pay_installment(&plan_id, &owner);

        let plan = client.get_installment_plan(&plan_id).unwrap();
        let amount = total / installments as i128;
        assert_eq!(plan.paid, amount);
        assert_eq!(plan.remaining, total - amount);
        assert_eq!(plan.next_due, plan_before.next_due + MONTH);
        assert_eq!(plan.status, InstallmentStatus::Active);

        let token_client = token::Client::new(&env, &token_address);
        assert_eq!(token_client.balance(&owner), total - amount);

        let events = events_as_tuples(&env);
        assert!(events.iter().any(|(sym, _)| *sym == symbol_short!("inst_paid")));
    }

    #[test]
    fn test_pay_final_installment_completes_plan() {
        let (env, client, _admin, token_address) = setup_with_token();
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        let owner = Address::generate(&env);
        let _meter_id = allowlist_and_register(&env, &client, &owner);

        let total: i128 = 3_000_000;
        let installments: u32 = 3;
        let plan_id = client.create_installment_plan(&owner, &total, &installments);

        token_admin_client.mint(&owner, &total);

        for _ in 0..installments {
            let plan = client.get_installment_plan(&plan_id).unwrap();
            env.ledger().set_timestamp(plan.next_due);
            client.pay_installment(&plan_id, &owner);
        }

        let plan = client.get_installment_plan(&plan_id).unwrap();
        assert_eq!(plan.remaining, 0);
        assert_eq!(plan.paid, total);
        assert_eq!(plan.status, InstallmentStatus::Completed);

        let events = events_as_tuples(&env);
        assert!(events.iter().any(|(sym, _)| *sym == symbol_short!("inst_paid")));
    }

    #[test]
    fn test_process_due_installments_defaults() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let owner = Address::generate(&env);
        let _meter_id = allowlist_and_register(&env, &client, &owner);

        let plan_id = client.create_installment_plan(&owner, &3_000_000_i128, &3_u32);
        let plan = client.get_installment_plan(&plan_id).unwrap();

        let grace = client.get_installment_grace_secs();
        env.ledger().set_timestamp(plan.next_due + grace + 1);

        client.process_due_installments();

        let plan = client.get_installment_plan(&plan_id).unwrap();
        assert_eq!(plan.status, InstallmentStatus::Defaulted);

        let events = events_as_tuples(&env);
        assert!(events.iter().any(|(sym, _)| *sym == symbol_short!("inst_dflt")));
    }

    #[test]
    fn test_get_installment_plan_and_payer_installments() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let owner = Address::generate(&env);
        let _meter_id = allowlist_and_register(&env, &client, &owner);

        let plan_id = client.create_installment_plan(&owner, &3_000_000_i128, &3_u32);

        let plan = client.get_installment_plan(&plan_id).unwrap();
        assert_eq!(plan.payer, owner);
        assert_eq!(plan.total, 3_000_000);

        let payer_plans = client.get_payer_installments(&owner);
        assert_eq!(payer_plans.len(), 1);
        assert_eq!(payer_plans.get(0).unwrap(), plan_id);

        let missing = client.get_installment_plan(&999_u64);
        assert!(missing.is_none());
    }

    #[test]
    fn test_pay_installment_on_defaulted_plan() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let owner = Address::generate(&env);
        let _meter_id = allowlist_and_register(&env, &client, &owner);

        let plan_id = client.create_installment_plan(&owner, &3_000_000_i128, &3_u32);
        let plan = client.get_installment_plan(&plan_id).unwrap();

        let grace = client.get_installment_grace_secs();
        env.ledger().set_timestamp(plan.next_due + grace + 1);
        client.process_due_installments();

        let result = client.try_pay_installment(&plan_id, &owner);
        assert_eq!(result, Err(Ok(Error::InstallmentPlanDefaulted)));
    }

    #[test]
    fn test_pay_installment_on_completed_plan() {
        let (env, client, _admin, token_address) = setup_with_token();
        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        let owner = Address::generate(&env);
        let _meter_id = allowlist_and_register(&env, &client, &owner);

        let total: i128 = 3_000_000;
        let installments: u32 = 3;
        let plan_id = client.create_installment_plan(&owner, &total, &installments);
        token_admin_client.mint(&owner, &total);

        for _ in 0..installments {
            let plan = client.get_installment_plan(&plan_id).unwrap();
            env.ledger().set_timestamp(plan.next_due);
            client.pay_installment(&plan_id, &owner);
        }

        let result = client.try_pay_installment(&plan_id, &owner);
        assert_eq!(result, Err(Ok(Error::InstallmentPlanCompleted)));
    }
}