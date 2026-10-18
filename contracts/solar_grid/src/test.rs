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
    // Delegation tests
    // ------------------------------------------------------------------

    fn setup_meter_with_payment(
        env: &Env,
        client: &SolarGridContractClient<'static>,
        token_address: &Address,
        meter_id: &String,
        owner: &Address,
    ) {
        let token_admin_client = token::StellarAssetClient::new(env, token_address);
        client.allowlist_add(owner);
        client.register_meter(meter_id, owner);
        token_admin_client.mint(owner, &10_000_000_i128);
        client.make_payment(meter_id, owner, &10_000_000_i128, &PaymentPlan::Daily);
    }

    #[test]
    fn test_add_delegate_with_each_permission() {
        let (env, client, _admin, token_address) = setup_with_token();
        let meter_id = String::from_str(&env, "SG-2024-METER1");
        let owner = Address::generate(&env);
        setup_meter_with_payment(&env, &client, &token_address, &meter_id, &owner);

        let expiry = env.ledger().timestamp() + 86_400;
        let view_only = Address::generate(&env);
        let payer = Address::generate(&env);
        let admin_delegate = Address::generate(&env);

        client.add_delegate(&meter_id, &owner, &view_only, &Permission::ViewOnly, &expiry);
        client.add_delegate(&meter_id, &owner, &payer, &Permission::Pay, &expiry);
        client.add_delegate(&meter_id, &owner, &admin_delegate, &Permission::Admin, &expiry);

        let delegates = client.get_delegates(&meter_id);
        assert_eq!(delegates.len(), 3);

        let mut found_view = false;
        let mut found_pay = false;
        let mut found_admin = false;
        for d in delegates.iter() {
            assert_eq!(d.expiry, expiry);
            match d.permissions {
                Permission::ViewOnly => found_view = true,
                Permission::Pay => found_pay = true,
                Permission::Admin => found_admin = true,
            }
        }
        assert!(found_view && found_pay && found_admin);
    }

    #[test]
    fn test_non_owner_add_delegate_unauthorized() {
        let (env, client, _admin, token_address) = setup_with_token();
        let meter_id = String::from_str(&env, "SG-2024-METER1");
        let owner = Address::generate(&env);
        setup_meter_with_payment(&env, &client, &token_address, &meter_id, &owner);

        let attacker = Address::generate(&env);
        let delegate = Address::generate(&env);
        let expiry = env.ledger().timestamp() + 86_400;

        let result = client.try_add_delegate(
            &meter_id,
            &attacker,
            &delegate,
            &Permission::ViewOnly,
            &expiry,
        );
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_add_delegate_zero_permissions_rejected() {
        let (env, client, _admin, token_address) = setup_with_token();
        let meter_id = String::from_str(&env, "SG-2024-METER1");
        let owner = Address::generate(&env);
        setup_meter_with_payment(&env, &client, &token_address, &meter_id, &owner);

        let delegate = Address::generate(&env);
        let expiry = env.ledger().timestamp() + 86_400;

        let result = client.try_add_delegate(
            &meter_id,
            &owner,
            &delegate,
            &Permission::None,
            &expiry,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_add_delegate_past_expiry_rejected() {
        let (env, client, _admin, token_address) = setup_with_token();
        let meter_id = String::from_str(&env, "SG-2024-METER1");
        let owner = Address::generate(&env);
        setup_meter_with_payment(&env, &client, &token_address, &meter_id, &owner);

        let delegate = Address::generate(&env);
        let past = env.ledger().timestamp().saturating_sub(1);

        let result = client.try_add_delegate(
            &meter_id,
            &owner,
            &delegate,
            &Permission::ViewOnly,
            &past,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_remove_delegate_removes_entry_and_emits_event() {
        let (env, client, _admin, token_address) = setup_with_token();
        let meter_id = String::from_str(&env, "SG-2024-METER1");
        let owner = Address::generate(&env);
        setup_meter_with_payment(&env, &client, &token_address, &meter_id, &owner);

        let delegate = Address::generate(&env);
        let expiry = env.ledger().timestamp() + 86_400;
        client.add_delegate(&meter_id, &owner, &delegate, &Permission::Pay, &expiry);
        assert_eq!(client.get_delegates(&meter_id).len(), 1);

        client.remove_delegate(&meter_id, &owner, &delegate);
        assert_eq!(client.get_delegates(&meter_id).len(), 0);

        let events = env.events().all();
        let removed_topic = (symbol_short!("deleg_rm"), meter_id.clone()).into_val(&env);
        let mut found = false;
        for (topics, _data) in events.iter() {
            if topics == removed_topic {
                found = true;
            }
        }
        assert!(found, "DelegateRemoved event not emitted");
    }

    #[test]
    fn test_expired_delegate_rejected_by_make_delegated_payment() {
        let (env, client, _admin, token_address) = setup_with_token();
        let meter_id = String::from_str(&env, "SG-2024-METER1");
        let owner = Address::generate(&env);
        setup_meter_with_payment(&env, &client, &token_address, &meter_id, &owner);

        let delegate = Address::generate(&env);
        let expiry = env.ledger().timestamp() + 100;
        client.add_delegate(&meter_id, &owner, &delegate, &Permission::Pay, &expiry);

        // Advance ledger past expiry
        env.ledger().set_timestamp(expiry + 1);

        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        token_admin_client.mint(&delegate, &1_000_000_i128);

        let result = client.try_make_delegated_payment(
            &meter_id,
            &delegate,
            &1_000_000_i128,
            &PaymentPlan::Daily,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_delegate_without_pay_permission_rejected() {
        let (env, client, _admin, token_address) = setup_with_token();
        let meter_id = String::from_str(&env, "SG-2024-METER1");
        let owner = Address::generate(&env);
        setup_meter_with_payment(&env, &client, &token_address, &meter_id, &owner);

        let delegate = Address::generate(&env);
        let expiry = env.ledger().timestamp() + 86_400;
        client.add_delegate(&meter_id, &owner, &delegate, &Permission::ViewOnly, &expiry);

        let token_admin_client = token::StellarAssetClient::new(&env, &token_address);
        token_admin_client.mint(&delegate, &1_000_000_i128);

        let result = client.try_make_delegated_payment(
            &meter_id,
            &delegate,
            &1_000_000_i128,
            &PaymentPlan::Daily,
        );
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_get_delegates_returns_full_structs() {
        let (env, client, _admin, token_address) = setup_with_token();
        let meter_id = String::from_str(&env, "SG-2024-METER1");
        let owner = Address::generate(&env);
        setup_meter_with_payment(&env, &client, &token_address, &meter_id, &owner);

        let delegate = Address::generate(&env);
        let expiry = env.ledger().timestamp() + 86_400;
        client.add_delegate(&meter_id, &owner, &delegate, &Permission::Admin, &expiry);

        let delegates = client.get_delegates(&meter_id);
        assert_eq!(delegates.len(), 1);
        let d = delegates.get(0).unwrap();
        assert_eq!(d.delegated_address, delegate);
        assert_eq!(d.permissions, Permission::Admin);
        assert_eq!(d.expiry, expiry);
    }

    #[test]
    fn test_delegate_added_event_topics() {
        let (env, client, _admin, token_address) = setup_with_token();
        let meter_id = String::from_str(&env, "SG-2024-METER1");
        let owner = Address::generate(&env);
        setup_meter_with_payment(&env, &client, &token_address, &meter_id, &owner);

        let delegate = Address::generate(&env);
        let expiry = env.ledger().timestamp() + 86_400;
        client.add_delegate(&meter_id, &owner, &delegate, &Permission::Pay, &expiry);

        let events = env.events().all();
        let added_topic = (symbol_short!("deleg_add"), meter_id.clone()).into_val(&env);
        let mut found = false;
        for (topics, _data) in events.iter() {
            if topics == added_topic {
                found = true;
            }
        }
        assert!(found, "DelegateAdded event not emitted");
    }

    // ------------------------------------------------------------------
    // Seasonal pricing tests
    // ------------------------------------------------------------------

    fn season_range(env: &Env, start: u64, end: u64, multiplier: u32) -> SeasonalRate {
        SeasonalRate {
            season: Season::Summer,
            start_date: start,
            end_date: end,
            multiplier,
        }
    }

    #[test]
    fn test_set_seasonal_rates_admin_only() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let attacker = Address::generate(&env);
        let now = env.ledger().timestamp();
        let rates = soroban_sdk::vec![
            &env,
            season_range(&env, now, now + 1_000, 12_000),
        ];

        let result = client.try_set_seasonal_rates(&attacker, &rates);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_set_seasonal_rates_overlapping_rejected() {
        let (env, client, admin, _token_address) = setup_with_token();
        let now = env.ledger().timestamp();
        let rates = soroban_sdk::vec![
            &env,
            season_range(&env, now, now + 1_000, 12_000),
            season_range(&env, now + 500, now + 2_000, 11_000),
        ];

        let result = client.try_set_seasonal_rates(&admin, &rates);
        assert!(result.is_err());
    }

    #[test]
    fn test_set_seasonal_rates_descending_rejected() {
        let (env, client, admin, _token_address) = setup_with_token();
        let now = env.ledger().timestamp();
        let rates = soroban_sdk::vec![
            &env,
            season_range(&env, now + 2_000, now + 1_000, 12_000),
        ];

        let result = client.try_set_seasonal_rates(&admin, &rates);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_current_season_matches_timestamp() {
        let (env, client, admin, _token_address) = setup_with_token();
        let now = env.ledger().timestamp();
        let rates = soroban_sdk::vec![
            &env,
            season_range(&env, now, now + 1_000, 12_000),
        ];
        client.set_seasonal_rates(&admin, &rates);

        env.ledger().set_timestamp(now + 500);
        let current = client.get_current_season();
        assert!(current.is_some());
        assert_eq!(current.unwrap().multiplier, 12_000);
    }

    #[test]
    fn test_compute_cost_applies_active_multiplier() {
        let (env, client, admin, _token_address) = setup_with_token();
        let now = env.ledger().timestamp();
        let rates = soroban_sdk::vec![
            &env,
            season_range(&env, now, now + 1_000, 20_000),
        ];
        client.set_seasonal_rates(&admin, &rates);
        env.ledger().set_timestamp(now + 100);

        let base = client.compute_cost(&1_000_i128);
        assert_eq!(base, 2_000_i128);
    }

    #[test]
    fn test_seasonal_rate_applied_event_emitted() {
        let (env, client, admin, _token_address) = setup_with_token();
        let now = env.ledger().timestamp();
        let rates = soroban_sdk::vec![
            &env,
            season_range(&env, now, now + 1_000, 15_000),
        ];
        client.set_seasonal_rates(&admin, &rates);

        let events = env.events().all();
        let applied_topic = symbol_short!("season_ap").into_val(&env);
        let mut found = false;
        for (topics, _data) in events.iter() {
            if topics == applied_topic {
                found = true;
            }
        }
        assert!(found, "SeasonalRateApplied event not emitted");
    }

    #[test]
    fn test_timestamp_outside_ranges_falls_back_to_base_rate() {
        let (env, client, admin, _token_address) = setup_with_token();
        let now = env.ledger().timestamp();
        let rates = soroban_sdk::vec![
            &env,
            season_range(&env, now + 10_000, now + 20_000, 20_000),
        ];
        client.set_seasonal_rates(&admin, &rates);

        env.ledger().set_timestamp(now);
        assert!(client.get_current_season().is_none());

        let base = client.compute_cost(&1_000_i128);
        assert_eq!(base, 1_000_i128);
    }

    // ------------------------------------------------------------------
    // Energy Certificate Authority tests
    // ------------------------------------------------------------------

    fn cert_setup() -> (Env, SolarGridContractClient<'static>, Address) {
        let (env, client, admin, _token_address) = setup_with_token();
        client.init_certificate_authority(&admin);
        (env, client, admin)
    }

    fn issue_sample_cert(
        env: &Env,
        client: &SolarGridContractClient<'static>,
        issuer: &Address,
        generator: &Address,
        serial: &str,
        energy: i128,
    ) -> u64 {
        let serial_id = String::from_str(env, serial);
        client.issue_certificate(
            issuer,
            generator,
            &serial_id,
            &energy,
            &EnergySource::Solar,
            &1_700_000_000_u64,
            &1_700_086_400_u64,
            &String::from_str(env, "US"),
            &String::from_str(env, "I-REC"),
        )
    }

    #[test]
    fn test_certificate_issue_and_verify_lifecycle() {
        let (env, client, admin) = cert_setup();
        let generator = Address::generate(&env);
        let verifier = Address::generate(&env);
        client.add_certificate_verifier(&admin, &verifier);

        let cert_id = issue_sample_cert(&env, &client, &admin, &generator, "CERT-0001", 1_000);

        let cert = client.get_certificate(&cert_id).unwrap();
        assert_eq!(cert.status, CertificateStatus::Pending);
        assert_eq!(cert.energy_kwh, 1_000);
        assert_eq!(cert.generator, generator);

        client.verify_certificate(&verifier, &cert_id);
        let verified = client.get_certificate(&cert_id).unwrap();
        assert_eq!(verified.status, CertificateStatus::Verified);
        assert_eq!(verified.verifier, Some(verifier));
    }

    #[test]
    fn test_certificate_third_party_verification_requires_registration() {
        let (env, client, admin) = cert_setup();
        let generator = Address::generate(&env);
        let rogue = Address::generate(&env);
        let cert_id = issue_sample_cert(&env, &client, &admin, &generator, "CERT-0002", 500);

        let result = client.try_verify_certificate(&rogue, &cert_id);
        assert_eq!(result, Err(Ok(Error::Unauthorized)));
    }

    #[test]
    fn test_certificate_standard_compliance_fields() {
        let (env, client, admin) = cert_setup();
        let generator = Address::generate(&env);
        let cert_id = issue_sample_cert(&env, &client, &admin, &generator, "CERT-0003", 750);

        let cert = client.get_certificate(&cert_id).unwrap();
        assert_eq!(cert.country, String::from_str(&env, "US"));
        assert_eq!(cert.standard, String::from_str(&env, "I-REC"));
        assert_eq!(cert.source, EnergySource::Solar);
        assert!(cert.proof_hash.len() > 0);
    }

    #[test]
    fn test_certificate_double_counting_prevented() {
        let (env, client, admin) = cert_setup();
        let generator = Address::generate(&env);
        let serial = String::from_str(&env, "CERT-DUP");
        client.issue_certificate(
            &admin,
            &generator,
            &serial,
            &1_000,
            &EnergySource::Solar,
            &1_700_000_000_u64,
            &1_700_086_400_u64,
            &String::from_str(&env, "US"),
            &String::from_str(&env, "I-REC"),
        );

        let result = client.try_issue_certificate(
            &admin,
            &generator,
            &serial,
            &1_000,
            &EnergySource::Solar,
            &1_700_000_000_u64,
            &1_700_086_400_u64,
            &String::from_str(&env, "US"),
            &String::from_str(&env, "I-REC"),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_certificate_revocation_blocks_transfer() {
        let (env, client, admin) = cert_setup();
        let generator = Address::generate(&env);
        let verifier = Address::generate(&env);
        let holder = Address::generate(&env);
        client.add_certificate_verifier(&admin, &verifier);

        let cert_id = issue_sample_cert(&env, &client, &admin, &generator, "CERT-0004", 1_000);
        client.verify_certificate(&verifier, &cert_id);
        client.transfer_certificate(&generator, &cert_id, &holder);

        client.revoke_certificate(&admin, &cert_id, &String::from_str(&env, "fraud"));

        let cert = client.get_certificate(&cert_id).unwrap();
        assert_eq!(cert.status, CertificateStatus::Revoked);

        let result = client.try_transfer_certificate(&holder, &cert_id, &generator);
        assert!(result.is_err());
    }

    #[test]
    fn test_certificate_revoked_cannot_be_verified() {
        let (env, client, admin) = cert_setup();
        let generator = Address::generate(&env);
        let verifier = Address::generate(&env);
        client.add_certificate_verifier(&admin, &verifier);

        let cert_id = issue_sample_cert(&env, &client, &admin, &generator, "CERT-0005", 1_000);
        client.revoke_certificate(&admin, &cert_id, &String::from_str(&env, "fraud"));

        let result = client.try_verify_certificate(&verifier, &cert_id);
        assert!(result.is_err());
    }

    // ------------------------------------------------------------------
    // Quantum-resistant cryptography tests
    // ------------------------------------------------------------------

    #[test]
    fn test_pqc_key_registration() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let user = Address::generate(&env);
        let pk = soroban_sdk::Bytes::from_array(&env, &[7u8; 32]);

        client.register_pqc_key(&user, &pk, &PqcAlgorithm::Dilithium3);
        let stored = client.get_pqc_key(&user).unwrap();
        assert_eq!(stored.algorithm, PqcAlgorithm::Dilithium3);
        assert_eq!(stored.public_key, pk);
    }

    #[test]
    fn test_pqc_dual_signing_migration() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let user = Address::generate(&env);
        let pk = soroban_sdk::Bytes::from_array(&env, &[3u8; 32]);
        client.register_pqc_key(&user, &pk, &PqcAlgorithm::Dilithium3);

        // Legacy-only signature still accepted during migration.
        let legacy = soroban_sdk::Bytes::from_array(&env, &[1u8; 64]);
        assert!(client.verify_dual_signature(&user, &legacy, &None));

        // PQC signature accepted.
        let pqc = soroban_sdk::Bytes::from_array(&env, &[2u8; 64]);
        assert!(client.verify_dual_signature(&user, &legacy, &Some(pqc)));
    }

    #[test]
    fn test_pqc_verification_rejects_tampered_signature() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let user = Address::generate(&env);
        let pk = soroban_sdk::Bytes::from_array(&env, &[9u8; 32]);
        client.register_pqc_key(&user, &pk, &PqcAlgorithm::Dilithium3);

        let empty = soroban_sdk::Bytes::new(&env);
        let tampered = soroban_sdk::Bytes::from_array(&env, &[0u8; 64]);
        assert!(!client.verify_dual_signature(&user, &empty, &Some(tampered)));
    }

    #[test]
    fn test_pqc_verification_without_registered_key_fails() {
        let (env, client, _admin, _token_address) = setup_with_token();
        let user = Address::generate(&env);
        let legacy = soroban_sdk::Bytes::from_array(&env, &[1u8; 64]);
        let pqc = soroban_sdk::Bytes::from_array(&env, &[2u8; 64]);
        assert!(!client.verify_dual_signature(&user, &legacy, &Some(pqc)));
    }

    // ------------------------------------------------------------------
    // Energy commons governance tests
    // ------------------------------------------------------------------

    fn commons_setup() -> (Env, SolarGridContractClient<'static>, Address) {
        let (env, client, admin, _token_address) = setup_with_token();
        client.init_commons(&admin, &10_000_i128);
        (env, client, admin)
    }

    #[test]
    fn test_commons_boundary_defined() {
        let (env, client, _admin) = commons_setup();
        let commons = client.get_commons().unwrap();
        assert_eq!(commons.total_capacity, 10_000);
        assert_eq!(commons.allocated, 0);
    }

    #[test]
    fn test_commons_member_rule_voting() {
        let (env, client, admin) = commons_setup();
        let member_a = Address::generate(&env);
        let member_b = Address::generate(&env);
        client.add_commons_member(&admin, &member_a);
        client.add_commons_member(&admin, &member_b);

        let rule_id = client.propose_commons_rule(&member_a, &String::from_str(&env, "max 2kW"));
        client.vote_commons_rule(&member_a, &rule_id, &true);
        client.vote_commons_rule(&member_b, &rule_id, &true);

        let rule = client.get_commons_rule(&rule_id).unwrap();
        assert!(rule.passed);
    }

    #[test]
    fn test_commons_monitoring_and_graduated_sanctions() {
        let (env, client, admin) = commons_setup();
        let member = Address::generate(&env);
        client.add_commons_member(&admin, &member);

        let first = client.report_commons_violation(&admin, &member);
        assert_eq!(first, SanctionLevel::Warning);

        let second = client.report_commons_violation(&admin, &member);
        assert_eq!(second, SanctionLevel::Restricted);

        let third = client.report_commons_violation(&admin, &member);
        assert_eq!(third, SanctionLevel::Suspended);
    }

    #[test]
    fn test_commons_conflict_resolution() {
        let (env, client, admin) = commons_setup();
        let member_a = Address::generate(&env);
        let member_b = Address::generate(&env);
        client.add_commons_member(&admin, &member_a);
        client.add_commons_member(&admin, &member_b);

        let conflict_id = client.open_commons_conflict(&member_a, &member_b);
        client.resolve_commons_conflict(&admin, &conflict_id, &member_a);

        let conflict = client.get_commons_conflict(&conflict_id).unwrap();
        assert!(conflict.resolved);
        assert_eq!(conflict.resolution, Some(member_a));
    }

    #[test]
    fn test_commons_allocation_within_boundary() {
        let (env, client, admin) = commons_setup();
        let member = Address::generate(&env);
        client.add_commons_member(&admin, &member);

        client.allocate_commons(&admin, &member, &4_000_i128);
        let commons = client.get_commons().unwrap();
        assert_eq!(commons.allocated, 4_000);

        let result = client.try_allocate_commons(&admin, &member, &7_000_i128);
        assert!(result.is_err());
    }
}