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

    // ------------------------------------------------------------------
    // Grid Export Credit System (Issue #990)
    // ------------------------------------------------------------------

    fn setup_export_meter(
        env: &Env,
        client: &SolarGridContractClient<'static>,
        admin: &Address,
        token_address: &Address,
    ) -> (String, Address) {
        let token_admin_client = token::StellarAssetClient::new(env, token_address);
        let meter_id = String::from_str(env, "SG-2024-EXPORT1");
        let owner = Address::generate(env);

        client.allowlist_add(&owner);
        client.register_meter(&meter_id, &owner);
        token_admin_client.mint(&owner, &10_000_000_i128);
        client.make_payment(&meter_id, &owner, &10_000_000_i128, &PaymentPlan::Daily);

        let _ = admin;
        (meter_id, owner)
    }

    #[test]
    fn test_set_and_get_export_rate() {
        let (env, client, admin, _token_address) = setup_with_token();

        // Default rate is zero before admin configures it.
        assert_eq!(client.get_export_rate(), 0_i128);

        client.set_export_rate(&admin, &250_i128);
        assert_eq!(client.get_export_rate(), 250_i128);

        // Admin can update the rate.
        client.set_export_rate(&admin, &500_i128);
        assert_eq!(client.get_export_rate(), 500_i128);
    }

    #[test]
    fn test_set_export_rate_rejects_zero_and_negative() {
        let (_env, client, admin, _token_address) = setup_with_token();

        let zero = std::panic::catch_unwind(|| {
            client.set_export_rate(&admin, &0_i128);
        });
        assert!(zero.is_err());

        let negative = std::panic::catch_unwind(|| {
            client.set_export_rate(&admin, &-10_i128);
        });
        assert!(negative.is_err());

        // Rate remains unchanged after rejected attempts.
        assert_eq!(client.get_export_rate(), 0_i128);
    }

    #[test]
    fn test_record_grid_export_credits_owner() {
        let (env, client, admin, token_address) = setup_with_token();
        let (meter_id, owner) = setup_export_meter(&env, &client, &admin, &token_address);

        client.set_export_rate(&admin, &300_i128);

        let balance_before = client.get_meter(&meter_id).unwrap().balance;

        env.mock_all_auths_allowing_non_root_auth();
        let credit = client.record_grid_export(&meter_id, &100_i128);

        // 100 kWh * 300 = 30_000 credits
        assert_eq!(credit, 30_000_i128);

        let meter = client.get_meter(&meter_id).unwrap();
        assert_eq!(meter.export_kwh, 100_i128);
        assert_eq!(meter.balance, balance_before + 30_000_i128);
        assert_eq!(meter.owner, owner);
    }

    #[test]
    fn test_record_grid_export_accumulates() {
        let (env, client, admin, token_address) = setup_with_token();
        let (meter_id, _owner) = setup_export_meter(&env, &client, &admin, &token_address);

        client.set_export_rate(&admin, &100_i128);
        env.mock_all_auths_allowing_non_root_auth();

        client.record_grid_export(&meter_id, &50_i128);
        client.record_grid_export(&meter_id, &25_i128);

        let meter = client.get_meter(&meter_id).unwrap();
        assert_eq!(meter.export_kwh, 75_i128);
    }

    #[test]
    fn test_record_grid_export_emits_event() {
        let (env, client, admin, token_address) = setup_with_token();
        let (meter_id, _owner) = setup_export_meter(&env, &client, &admin, &token_address);

        client.set_export_rate(&admin, &200_i128);
        env.mock_all_auths_allowing_non_root_auth();

        client.record_grid_export(&meter_id, &10_i128);

        let events = env.events().all();
        let mut found = false;
        for (_contract, topics, data) in events.iter() {
            if topics.len() >= 1 {
                if let Ok(sym) = Symbol::try_from_val(&env, &topics.get(0).unwrap()) {
                    if sym == symbol_short!("GridExp") {
                        let (kwh, credit): (i128, i128) =
                            <(i128, i128)>::try_from_val(&env, &data).unwrap();
                        assert_eq!(kwh, 10_i128);
                        assert_eq!(credit, 2_000_i128);
                        found = true;
                    }
                }
            }
        }
        assert!(found, "GridExport event not emitted");
    }

    #[test]
    fn test_record_grid_export_rejects_unknown_meter() {
        let (env, client, admin, _token_address) = setup_with_token();
        client.set_export_rate(&admin, &100_i128);

        let unknown = String::from_str(&env, "SG-UNKNOWN");
        env.mock_all_auths_allowing_non_root_auth();

        let result = std::panic::catch_unwind(|| {
            client.record_grid_export(&unknown, &10_i128);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_record_grid_export_rejects_non_positive_kwh() {
        let (env, client, admin, token_address) = setup_with_token();
        let (meter_id, _owner) = setup_export_meter(&env, &client, &admin, &token_address);
        client.set_export_rate(&admin, &100_i128);
        env.mock_all_auths_allowing_non_root_auth();

        let zero = std::panic::catch_unwind(|| {
            client.record_grid_export(&meter_id, &0_i128);
        });
        assert!(zero.is_err());

        let negative = std::panic::catch_unwind(|| {
            client.record_grid_export(&meter_id, &-5_i128);
        });
        assert!(negative.is_err());
    }

    #[test]
    fn test_get_export_history_pagination_and_cap() {
        let (env, client, admin, token_address) = setup_with_token();
        let (meter_id, _owner) = setup_export_meter(&env, &client, &admin, &token_address);
        client.set_export_rate(&admin, &100_i128);
        env.mock_all_auths_allowing_non_root_auth();

        for _ in 0..5 {
            client.record_grid_export(&meter_id, &10_i128);
        }

        let all = client.get_export_history(&meter_id, &0_u32, &10_u32);
        assert_eq!(all.len(), 5);

        let page = client.get_export_history(&meter_id, &1_u32, &2_u32);
        assert_eq!(page.len(), 2);

        // Offset beyond the end returns an empty page.
        let empty = client.get_export_history(&meter_id, &100_u32, &10_u32);
        assert_eq!(empty.len(), 0);

        // Limit is capped by the contract; requesting a huge limit is safe.
        let capped = client.get_export_history(&meter_id, &0_u32, &10_000_u32);
        assert!(capped.len() <= 5);
    }

    #[test]
    fn test_get_export_totals_separate_counters() {
        let (env, client, admin, token_address) = setup_with_token();
        let (meter_id, _owner) = setup_export_meter(&env, &client, &admin, &token_address);
        client.set_export_rate(&admin, &100_i128);
        env.mock_all_auths_allowing_non_root_auth();

        client.record_grid_export(&meter_id, &40_i128);
        client.record_grid_export(&meter_id, &60_i128);

        let (import_kwh, export_kwh) = client.get_export_totals(&meter_id);
        assert_eq!(export_kwh, 100_i128);
        assert_eq!(import_kwh, 0_i128);
    }

    #[test]
    fn test_import_and_export_counters_independent() {
        let (env, client, admin, token_address) = setup_with_token();
        let (meter_id, _owner) = setup_export_meter(&env, &client, &admin, &token_address);
        client.set_export_rate(&admin, &100_i128);
        env.mock_all_auths_allowing_non_root_auth();

        // Import usage should not affect export counters.
        client.update_usage(&meter_id, &500_i128);
        let (import_kwh, export_kwh) = client.get_export_totals(&meter_id);
        assert_eq!(import_kwh, 500_i128);
        assert_eq!(export_kwh, 0_i128);

        // Export should not affect import counters.
        client.record_grid_export(&meter_id, &30_i128);
        let (import_kwh2, export_kwh2) = client.get_export_totals(&meter_id);
        assert_eq!(import_kwh2, 500_i128);
        assert_eq!(export_kwh2, 30_i128);
    }
}