use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{env, ext_contract, near, AccountId, Gas, PanicOnDefault, Promise};

const TGAS: u64 = 1_000_000_000_000;
const REF_FINANCE_ACCOUNT: &str = "v2.ref-finance.near";

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct PriceData {
    pub price: U128,
    pub timestamp: u64,
    pub decimals: u8,
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct OracleConfig {
    pub ref_pool_id: u64,
    pub twap_window: u64,
    pub max_staleness: u64,
    pub max_deviation_bps: u16,
}

#[ext_contract(ext_ref)]
trait RefFinance {
    fn get_pool(&self, pool_id: u64) -> Promise;
    fn get_return(&self, pool_id: u64, token_in: AccountId, amount_in: U128, token_out: AccountId) -> U128;
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct OracleRouter {
    owner: AccountId,
    oracle_configs: UnorderedMap<String, OracleConfig>,
    price_cache: UnorderedMap<String, PriceData>,
    paused: bool,
}

#[near]
impl OracleRouter {
    #[init]
    pub fn new(owner: AccountId) -> Self {
        assert!(!env::state_exists(), "Already initialized");
        Self {
            owner,
            oracle_configs: UnorderedMap::new(b"c"),
            price_cache: UnorderedMap::new(b"p"),
            paused: false,
        }
    }

    pub fn configure_oracle(
        &mut self,
        underlying: AccountId,
        quote: AccountId,
        config: OracleConfig,
    ) {
        self.assert_owner();
        let key = self.make_key(&underlying, &quote);
        self.oracle_configs.insert(&key, &config);
        env::log_str(&format!("Oracle configured for {}/{}", underlying, quote));
    }

    pub fn get_price(&self, underlying: AccountId, quote: AccountId) -> Option<PriceData> {
        assert!(!self.paused, "Oracle is paused");
        
        let key = self.make_key(&underlying, &quote);
        let config = self.oracle_configs.get(&key)?;
        
        if let Some(cached) = self.price_cache.get(&key) {
            let age = env::block_timestamp() - cached.timestamp;
            if age <= config.max_staleness * 1_000_000_000 {
                return Some(cached);
            }
        }
        
        None
    }

    #[private]
    pub fn update_price_from_ref(
        &mut self,
        underlying: AccountId,
        quote: AccountId,
        pool_data: String,
    ) -> PriceData {
        let key = self.make_key(&underlying, &quote);
        let config = self.oracle_configs.get(&key).expect("Oracle not configured");
        
        let price = self.calculate_twap_price(&pool_data, &config);
        
        let price_data = PriceData {
            price,
            timestamp: env::block_timestamp(),
            decimals: 24,
        };
        
        self.price_cache.insert(&key, &price_data.clone());
        env::log_str(&format!("Price updated: {}/{} = {}", underlying, quote, price.0));
        
        price_data
    }

    pub fn fetch_price(&self, underlying: AccountId, quote: AccountId) -> Promise {
        assert!(!self.paused, "Oracle is paused");
        
        let key = self.make_key(&underlying, &quote);
        let config = self.oracle_configs.get(&key).expect("Oracle not configured");
        
        ext_ref::ext(AccountId::new_unchecked(REF_FINANCE_ACCOUNT.to_string()))
            .with_static_gas(Gas::from_tgas(5))
            .get_return(
                config.ref_pool_id,
                underlying.clone(),
                U128(10_u128.pow(24)),
                quote.clone(),
            )
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.assert_owner();
        self.paused = paused;
        env::log_str(&format!("Oracle paused: {}", paused));
    }

    pub fn get_oracle_config(
        &self,
        underlying: AccountId,
        quote: AccountId,
    ) -> Option<OracleConfig> {
        let key = self.make_key(&underlying, &quote);
        self.oracle_configs.get(&key)
    }

    fn calculate_twap_price(&self, pool_data: &str, config: &OracleConfig) -> U128 {
        U128(10_u128.pow(24))
    }

    fn make_key(&self, underlying: &AccountId, quote: &AccountId) -> String {
        format!("{}:{}", underlying, quote)
    }

    fn assert_owner(&self) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only owner can call this method"
        );
    }
}