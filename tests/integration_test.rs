#[cfg(test)]
mod tests {
    use near_sdk::json_types::U128;
    use near_sdk::serde_json::json;
    use near_sdk::AccountId;
    use near_sdk_sim::{deploy, init_simulator, to_yocto, ContractAccount, UserAccount};

    const FORWARD_FACTORY_WASM: &[u8] = include_bytes!("../res/forward-factory.wasm");
    const FORWARD_MARKET_WASM: &[u8] = include_bytes!("../res/forward-market.wasm");
    const LONG_TOKEN_WASM: &[u8] = include_bytes!("../res/long-token.wasm");
    const SHORT_TOKEN_WASM: &[u8] = include_bytes!("../res/short-token.wasm");
    const FEE_COLLECTOR_WASM: &[u8] = include_bytes!("../res/fee-collector.wasm");
    const ORACLE_ROUTER_WASM: &[u8] = include_bytes!("../res/oracle-router.wasm");

    fn init() -> (UserAccount, ContractAccount, ContractAccount, ContractAccount) {
        let root = init_simulator(None);
        
        let oracle = deploy!(
            contract: ORACLE_ROUTER_WASM,
            contract_id: "oracle".to_string(),
            bytes: &ORACLE_ROUTER_WASM,
            signer_account: root,
            init_method: new(root.account_id())
        );

        let fee_collector = deploy!(
            contract: FEE_COLLECTOR_WASM,
            contract_id: "fee_collector".to_string(),
            bytes: &FEE_COLLECTOR_WASM,
            signer_account: root,
            init_method: new(root.account_id(), root.account_id())
        );

        let factory = deploy!(
            contract: FORWARD_FACTORY_WASM,
            contract_id: "factory".to_string(),
            bytes: &FORWARD_FACTORY_WASM,
            signer_account: root,
            init_method: new(
                root.account_id(),
                oracle.account_id(),
                fee_collector.account_id(),
                root.account_id()
            )
        );

        (root, factory, oracle, fee_collector)
    }

    #[test]
    fn test_deploy_market() {
        let (root, factory, oracle, fee_collector) = init();
        
        let res = root.call(
            factory.account_id(),
            "set_contract_codes",
            &json!({
                "market_code": FORWARD_MARKET_WASM.to_vec(),
                "long_token_code": LONG_TOKEN_WASM.to_vec(),
                "short_token_code": SHORT_TOKEN_WASM.to_vec(),
            }).to_string().into_bytes(),
            near_sdk_sim::DEFAULT_GAS,
            0,
        );
        assert!(res.is_ok());

        let maturity = 1700000000u64;
        let params = json!({
            "underlying": "wrap.near",
            "quote": "usdc.near",
            "maturity": maturity,
            "strike_k": U128(50_000_000_000_000_000_000_000_000u128),
            "lower_bound_l": U128(30_000_000_000_000_000_000_000_000u128),
            "upper_bound_u": U128(70_000_000_000_000_000_000_000_000u128),
            "mint_fee_bps": 30,
            "settle_fee_bps": 50,
            "redeem_fee_bps": 20,
        });

        let res = root.call(
            factory.account_id(),
            "deploy_market",
            &json!({ "params": params }).to_string().into_bytes(),
            near_sdk_sim::DEFAULT_GAS,
            to_yocto("5"),
        );
        assert!(res.is_ok());

        let market_count: u64 = root
            .view(factory.account_id(), "get_market_count", &[])
            .unwrap_json();
        assert_eq!(market_count, 1);
    }

    #[test]
    fn test_market_lifecycle() {
        let (root, factory, oracle, fee_collector) = init();
        
        root.call(
            factory.account_id(),
            "set_contract_codes",
            &json!({
                "market_code": FORWARD_MARKET_WASM.to_vec(),
                "long_token_code": LONG_TOKEN_WASM.to_vec(),
                "short_token_code": SHORT_TOKEN_WASM.to_vec(),
            }).to_string().into_bytes(),
            near_sdk_sim::DEFAULT_GAS,
            0,
        );

        let params = json!({
            "underlying": "wrap.near",
            "quote": "usdc.near",
            "maturity": 1700000000u64,
            "strike_k": U128(50_000_000_000_000_000_000_000_000u128),
            "lower_bound_l": U128(30_000_000_000_000_000_000_000_000u128),
            "upper_bound_u": U128(70_000_000_000_000_000_000_000_000u128),
            "mint_fee_bps": 30,
            "settle_fee_bps": 50,
            "redeem_fee_bps": 20,
        });

        root.call(
            factory.account_id(),
            "deploy_market",
            &json!({ "params": params }).to_string().into_bytes(),
            near_sdk_sim::DEFAULT_GAS,
            to_yocto("5"),
        );

        let markets: Vec<serde_json::Value> = root
            .view(
                factory.account_id(),
                "get_all_markets",
                &json!({ "from_index": 0, "limit": 10 }).to_string().into_bytes(),
            )
            .unwrap_json();
        
        assert_eq!(markets.len(), 1);
        
        let market_info = &markets[0];
        let market_id = market_info["market_id"].as_str().unwrap();
        
        println!("Deployed market at: {}", market_id);
    }

    #[test]
    fn test_settlement_factor_calculation() {
        let lower = 30_000_000_000_000_000_000_000_000u128;
        let upper = 70_000_000_000_000_000_000_000_000u128;
        let one = 1_000_000_000_000_000_000_000_000u128;
        
        fn calculate_factor(price: u128, lower: u128, upper: u128, one: u128) -> u128 {
            if price <= lower {
                0
            } else if price >= upper {
                one
            } else {
                ((price - lower) * one) / (upper - lower)
            }
        }
        
        assert_eq!(calculate_factor(20_000_000_000_000_000_000_000_000, lower, upper, one), 0);
        
        assert_eq!(calculate_factor(30_000_000_000_000_000_000_000_000, lower, upper, one), 0);
        
        assert_eq!(calculate_factor(50_000_000_000_000_000_000_000_000, lower, upper, one), 500_000_000_000_000_000_000_000);
        
        assert_eq!(calculate_factor(70_000_000_000_000_000_000_000_000, lower, upper, one), one);
        
        assert_eq!(calculate_factor(80_000_000_000_000_000_000_000_000, lower, upper, one), one);
    }

    #[test]
    fn test_fee_calculations() {
        let amount = 1_000_000_000_000_000_000_000_000u128;
        
        let mint_fee_bps = 30u128;
        let mint_fee = (amount * mint_fee_bps) / 10000;
        assert_eq!(mint_fee, 3_000_000_000_000_000_000_000);
        
        let settle_fee_bps = 50u128;
        let settle_fee = (amount * settle_fee_bps) / 10000;
        assert_eq!(settle_fee, 5_000_000_000_000_000_000_000);
        
        let redeem_fee_bps = 20u128;
        let redeem_fee = (amount * redeem_fee_bps) / 10000;
        assert_eq!(redeem_fee, 2_000_000_000_000_000_000_000);
    }
}