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
    // Community solar farm tests
    // ------------------------------------------------------------------

    fn setup_solar_farm(
        env: &Env,
        client: &SolarGridContractClient<'static>,
        farm_id: &str,
        capacity: i128,
    ) {
        let id = String::from_str(env, farm_id);
        client.create_solar_farm(&id, &capacity);
    }

    #[test]
    fn test_create_solar_farm() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let farm_id = String::from_str(&env, "FARM-001");

        client.create_solar_farm(&farm_id, &10_000_i128);

        let farm = client.get_solar_farm(&farm_id).unwrap();
        assert_eq!(farm.capacity, 10_000_i128);
        assert_eq!(farm.total_generation, 0_i128);
        assert_eq!(farm.shareholders.len(), 0);
    }

    #[test]
    fn test_buy_solar_share() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let farm_id = String::from_str(&env, "FARM-001");
        let investor = Address::generate(&env);

        client.create_solar_farm(&farm_id, &10_000_i128);
        client.buy_solar_share(&farm_id, &investor, &2_500_i128);

        let farm = client.get_solar_farm(&farm_id).unwrap();
        assert_eq!(farm.shareholders.len(), 1);
        assert_eq!(client.get_solar_share(&farm_id, &investor), 2_500_i128);
    }

    #[test]
    fn test_buy_solar_share_zero_amount_fails() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let farm_id = String::from_str(&env, "FARM-001");
        let investor = Address::generate(&env);

        client.create_solar_farm(&farm_id, &10_000_i128);

        let result = std::panic::catch_unwind(|| {
            client.buy_solar_share(&farm_id, &investor, &0_i128);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_buy_solar_share_negative_amount_fails() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let farm_id = String::from_str(&env, "FARM-001");
        let investor = Address::generate(&env);

        client.create_solar_farm(&farm_id, &10_000_i128);

        let result = std::panic::catch_unwind(|| {
            client.buy_solar_share(&farm_id, &investor, &-100_i128);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_buy_solar_share_unknown_farm_fails() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let farm_id = String::from_str(&env, "FARM-UNKNOWN");
        let investor = Address::generate(&env);

        let result = std::panic::catch_unwind(|| {
            client.buy_solar_share(&farm_id, &investor, &1_000_i128);
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_shareholders_balances() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let farm_id = String::from_str(&env, "FARM-001");
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.create_solar_farm(&farm_id, &10_000_i128);
        client.buy_solar_share(&farm_id, &alice, &3_000_i128);
        client.buy_solar_share(&farm_id, &bob, &1_000_i128);
        // Alice tops up her position
        client.buy_solar_share(&farm_id, &alice, &1_000_i128);

        assert_eq!(client.get_solar_share(&farm_id, &alice), 4_000_i128);
        assert_eq!(client.get_solar_share(&farm_id, &bob), 1_000_i128);

        let farm = client.get_solar_farm(&farm_id).unwrap();
        assert_eq!(farm.shareholders.len(), 2);
    }

    #[test]
    fn test_distribute_generation_credits_proportionally() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let farm_id = String::from_str(&env, "FARM-001");
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.create_solar_farm(&farm_id, &10_000_i128);
        client.buy_solar_share(&farm_id, &alice, &3_000_i128);
        client.buy_solar_share(&farm_id, &bob, &1_000_i128);

        // 4_000 total shares; 8_000 generation units.
        // Alice (75%) -> 6_000, Bob (25%) -> 2_000.
        client.distribute_generation_credits(&farm_id, &8_000_i128);

        assert_eq!(client.get_solar_credits(&farm_id, &alice), 6_000_i128);
        assert_eq!(client.get_solar_credits(&farm_id, &bob), 2_000_i128);
    }

    #[test]
    fn test_total_generation_tracked_across_distributions() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let farm_id = String::from_str(&env, "FARM-001");
        let alice = Address::generate(&env);

        client.create_solar_farm(&farm_id, &10_000_i128);
        client.buy_solar_share(&farm_id, &alice, &1_000_i128);

        client.distribute_generation_credits(&farm_id, &5_000_i128);
        client.distribute_generation_credits(&farm_id, &2_500_i128);

        let farm = client.get_solar_farm(&farm_id).unwrap();
        assert_eq!(farm.total_generation, 7_500_i128);
        assert_eq!(client.get_solar_credits(&farm_id, &alice), 7_500_i128);
    }

    #[test]
    fn test_share_purchased_event_emitted() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let farm_id = String::from_str(&env, "FARM-001");
        let investor = Address::generate(&env);

        client.create_solar_farm(&farm_id, &10_000_i128);
        client.buy_solar_share(&farm_id, &investor, &2_500_i128);

        let events = env.events().all();
        let expected_topic = symbol_short!("SharePur");
        let mut found = false;
        for (contract, topics, data) in events.iter() {
            if contract != client.address {
                continue;
            }
            if topics.len() == 1 {
                let topic: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
                if topic == expected_topic {
                    let (emitted_farm, emitted_investor, emitted_amount): (String, Address, i128) =
                        <(String, Address, i128)>::try_from_val(&env, &data).unwrap();
                    assert_eq!(emitted_farm, farm_id);
                    assert_eq!(emitted_investor, investor);
                    assert_eq!(emitted_amount, 2_500_i128);
                    found = true;
                }
            }
        }
        assert!(found, "SharePurchased event not emitted");
    }

    #[test]
    fn test_credits_distributed_event_emitted() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let farm_id = String::from_str(&env, "FARM-001");
        let alice = Address::generate(&env);

        client.create_solar_farm(&farm_id, &10_000_i128);
        client.buy_solar_share(&farm_id, &alice, &1_000_i128);
        client.distribute_generation_credits(&farm_id, &4_000_i128);

        let events = env.events().all();
        let expected_topic = symbol_short!("CredDist");
        let mut found = false;
        for (contract, topics, data) in events.iter() {
            if contract != client.address {
                continue;
            }
            if topics.len() == 1 {
                let topic: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
                if topic == expected_topic {
                    let (emitted_farm, emitted_amount): (String, i128) =
                        <(String, i128)>::try_from_val(&env, &data).unwrap();
                    assert_eq!(emitted_farm, farm_id);
                    assert_eq!(emitted_amount, 4_000_i128);
                    found = true;
                }
            }
        }
        assert!(found, "CreditsDistributed event not emitted");
    }
}