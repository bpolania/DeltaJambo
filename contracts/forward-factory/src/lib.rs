use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::collections::{UnorderedMap, UnorderedSet};
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{env, ext_contract, near, require, AccountId, Balance, Gas, PanicOnDefault, Promise, PublicKey};

const TGAS: u64 = 1_000_000_000_000;
const DEPLOY_GAS: Gas = Gas::from_tgas(100);
const CALLBACK_GAS: Gas = Gas::from_tgas(10);
const MARKET_STORAGE: Balance = 10_000_000_000_000_000_000_000_000;
const TOKEN_STORAGE: Balance = 5_000_000_000_000_000_000_000_000;

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct MarketParams {
    pub underlying: AccountId,
    pub quote: AccountId,
    pub maturity: u64,
    pub strike_k: U128,
    pub lower_bound_l: U128,
    pub upper_bound_u: U128,
    pub mint_fee_bps: u16,
    pub settle_fee_bps: u16,
    pub redeem_fee_bps: u16,
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct MarketInfo {
    pub market_id: AccountId,
    pub long_token: AccountId,
    pub short_token: AccountId,
    pub params: MarketParams,
    pub created_at: u64,
    pub creator: AccountId,
}

#[ext_contract(ext_self)]
trait SelfCallback {
    fn on_market_deployed(&mut self, market_key: String, market_info: MarketInfo);
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct ForwardFactory {
    owner: AccountId,
    oracle: AccountId,
    fee_collector: AccountId,
    guardian: AccountId,
    markets: UnorderedMap<String, MarketInfo>,
    markets_by_creator: UnorderedMap<AccountId, Vec<String>>,
    all_market_keys: UnorderedSet<String>,
    market_code: Vec<u8>,
    long_token_code: Vec<u8>,
    short_token_code: Vec<u8>,
    paused: bool,
    deploy_counter: u64,
}

#[near]
impl ForwardFactory {
    #[init]
    pub fn new(
        owner: AccountId,
        oracle: AccountId,
        fee_collector: AccountId,
        guardian: AccountId,
    ) -> Self {
        assert!(!env::state_exists(), "Already initialized");
        Self {
            owner,
            oracle,
            fee_collector,
            guardian,
            markets: UnorderedMap::new(b"m"),
            markets_by_creator: UnorderedMap::new(b"c"),
            all_market_keys: UnorderedSet::new(b"k"),
            market_code: Vec::new(),
            long_token_code: Vec::new(),
            short_token_code: Vec::new(),
            paused: false,
            deploy_counter: 0,
        }
    }

    pub fn set_contract_codes(
        &mut self,
        market_code: Vec<u8>,
        long_token_code: Vec<u8>,
        short_token_code: Vec<u8>,
    ) {
        self.assert_owner();
        self.market_code = market_code;
        self.long_token_code = long_token_code;
        self.short_token_code = short_token_code;
        env::log_str("Contract codes updated");
    }

    #[payable]
    pub fn deploy_market(&mut self, params: MarketParams) -> Promise {
        require!(!self.paused, "Factory is paused");
        require!(!self.market_code.is_empty(), "Market code not set");
        require!(!self.long_token_code.is_empty(), "Token codes not set");
        
        let deposit = env::attached_deposit();
        require!(
            deposit >= MARKET_STORAGE + 2 * TOKEN_STORAGE,
            "Insufficient deposit for deployment"
        );
        
        let market_key = self.compute_market_key(&params);
        require!(!self.markets.contains_key(&market_key), "Market already exists");
        
        let creator = env::predecessor_account_id();
        self.deploy_counter += 1;
        
        let market_id = AccountId::new_unchecked(format!(
            "market-{}.{}",
            self.deploy_counter,
            env::current_account_id()
        ));
        
        let long_token_id = AccountId::new_unchecked(format!(
            "long-{}.{}",
            self.deploy_counter,
            env::current_account_id()
        ));
        
        let short_token_id = AccountId::new_unchecked(format!(
            "short-{}.{}",
            self.deploy_counter,
            env::current_account_id()
        ));
        
        let long_name = format!("LONG-{}", params.underlying);
        let short_name = format!("SHORT-{}", params.underlying);
        let decimals = 24u8;
        
        Promise::new(long_token_id.clone())
            .create_account()
            .transfer(TOKEN_STORAGE)
            .deploy_contract(self.long_token_code.clone())
            .function_call(
                "new".to_string(),
                near_sdk::serde_json::json!({
                    "market": market_id,
                    "name": long_name,
                    "symbol": "LONG",
                    "decimals": decimals
                }).to_string().into_bytes(),
                0,
                Gas::from_tgas(30),
            )
            .then(
                Promise::new(short_token_id.clone())
                    .create_account()
                    .transfer(TOKEN_STORAGE)
                    .deploy_contract(self.short_token_code.clone())
                    .function_call(
                        "new".to_string(),
                        near_sdk::serde_json::json!({
                            "market": market_id,
                            "name": short_name,
                            "symbol": "SHORT",
                            "decimals": decimals
                        }).to_string().into_bytes(),
                        0,
                        Gas::from_tgas(30),
                    )
            )
            .then(
                Promise::new(market_id.clone())
                    .create_account()
                    .transfer(MARKET_STORAGE)
                    .deploy_contract(self.market_code.clone())
                    .function_call(
                        "new".to_string(),
                        near_sdk::serde_json::json!({
                            "params": params,
                            "long_token": long_token_id,
                            "short_token": short_token_id,
                            "oracle": self.oracle,
                            "fee_collector": self.fee_collector,
                            "owner": self.owner,
                            "guardian": self.guardian
                        }).to_string().into_bytes(),
                        0,
                        Gas::from_tgas(30),
                    )
            )
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(CALLBACK_GAS)
                    .on_market_deployed(
                        market_key.clone(),
                        MarketInfo {
                            market_id,
                            long_token: long_token_id,
                            short_token: short_token_id,
                            params,
                            created_at: env::block_timestamp(),
                            creator,
                        }
                    )
            )
    }

    #[private]
    pub fn on_market_deployed(&mut self, market_key: String, market_info: MarketInfo) {
        self.markets.insert(&market_key, &market_info);
        self.all_market_keys.insert(&market_key);
        
        let mut creator_markets = self.markets_by_creator
            .get(&market_info.creator)
            .unwrap_or_else(Vec::new);
        creator_markets.push(market_key.clone());
        self.markets_by_creator.insert(&market_info.creator, &creator_markets);
        
        env::log_str(&format!(
            "Market deployed: {} at {}",
            market_key, market_info.market_id
        ));
    }

    pub fn get_market(&self, market_key: String) -> Option<MarketInfo> {
        self.markets.get(&market_key)
    }

    pub fn get_market_by_params(&self, params: MarketParams) -> Option<MarketInfo> {
        let key = self.compute_market_key(&params);
        self.markets.get(&key)
    }

    pub fn get_markets_by_creator(&self, creator: AccountId) -> Vec<MarketInfo> {
        self.markets_by_creator
            .get(&creator)
            .unwrap_or_else(Vec::new)
            .iter()
            .filter_map(|key| self.markets.get(key))
            .collect()
    }

    pub fn get_all_markets(&self, from_index: u64, limit: u64) -> Vec<MarketInfo> {
        let keys: Vec<String> = self.all_market_keys.iter().collect();
        keys.iter()
            .skip(from_index as usize)
            .take(limit as usize)
            .filter_map(|key| self.markets.get(key))
            .collect()
    }

    pub fn get_market_count(&self) -> u64 {
        self.all_market_keys.len()
    }

    pub fn set_paused(&mut self, paused: bool) {
        require!(
            env::predecessor_account_id() == self.guardian || env::predecessor_account_id() == self.owner,
            "Not authorized"
        );
        self.paused = paused;
        env::log_str(&format!("Factory paused: {}", paused));
    }

    pub fn update_oracle(&mut self, oracle: AccountId) {
        self.assert_owner();
        self.oracle = oracle;
        env::log_str(&format!("Oracle updated to {}", self.oracle));
    }

    pub fn update_fee_collector(&mut self, fee_collector: AccountId) {
        self.assert_owner();
        self.fee_collector = fee_collector;
        env::log_str(&format!("Fee collector updated to {}", self.fee_collector));
    }

    pub fn update_guardian(&mut self, guardian: AccountId) {
        self.assert_owner();
        self.guardian = guardian;
        env::log_str(&format!("Guardian updated to {}", self.guardian));
    }

    fn compute_market_key(&self, params: &MarketParams) -> String {
        format!(
            "{}:{}:{}:{}:{}:{}",
            params.underlying,
            params.quote,
            params.maturity,
            params.strike_k.0,
            params.lower_bound_l.0,
            params.upper_bound_u.0
        )
    }

    fn assert_owner(&self) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only owner can call this method"
        );
    }
}