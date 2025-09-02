use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::json_types::U128;
use near_sdk::{env, ext_contract, near, AccountId, Balance, Gas, PanicOnDefault, Promise};

const TGAS: u64 = 1_000_000_000_000;
const FT_TRANSFER_GAS: Gas = Gas::from_tgas(10);

#[ext_contract(ext_ft)]
trait FungibleToken {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
    fn ft_balance_of(&self, account_id: AccountId) -> U128;
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct FeeCollector {
    owner: AccountId,
    treasury: AccountId,
    authorized_markets: UnorderedMap<AccountId, bool>,
    collected_fees: UnorderedMap<AccountId, Balance>,
}

#[near]
impl FeeCollector {
    #[init]
    pub fn new(owner: AccountId, treasury: AccountId) -> Self {
        assert!(!env::state_exists(), "Already initialized");
        Self {
            owner,
            treasury,
            authorized_markets: UnorderedMap::new(b"m"),
            collected_fees: UnorderedMap::new(b"f"),
        }
    }

    pub fn authorize_market(&mut self, market: AccountId) {
        self.assert_owner();
        self.authorized_markets.insert(&market, &true);
        env::log_str(&format!("Market {} authorized", market));
    }

    pub fn revoke_market(&mut self, market: AccountId) {
        self.assert_owner();
        self.authorized_markets.remove(&market);
        env::log_str(&format!("Market {} revoked", market));
    }

    pub fn set_treasury(&mut self, treasury: AccountId) {
        self.assert_owner();
        self.treasury = treasury;
        env::log_str(&format!("Treasury set to {}", treasury));
    }

    pub fn withdraw_fees(&mut self, token: AccountId, amount: Option<U128>) -> Promise {
        self.assert_owner();
        
        let collected = self.collected_fees.get(&token).unwrap_or(0);
        let withdraw_amount = amount.map(|a| a.0).unwrap_or(collected);
        
        assert!(withdraw_amount <= collected, "Insufficient collected fees");
        
        let new_balance = collected - withdraw_amount;
        if new_balance == 0 {
            self.collected_fees.remove(&token);
        } else {
            self.collected_fees.insert(&token, &new_balance);
        }

        ext_ft::ext(token)
            .with_static_gas(FT_TRANSFER_GAS)
            .ft_transfer(
                self.treasury.clone(),
                U128(withdraw_amount),
                Some("Fee withdrawal".to_string()),
            )
    }

    pub fn record_fee(&mut self, token: AccountId, amount: Balance) {
        self.assert_authorized_market();
        let current = self.collected_fees.get(&token).unwrap_or(0);
        self.collected_fees.insert(&token, &(current + amount));
        env::log_str(&format!("Recorded fee: {} of token {}", amount, token));
    }

    pub fn get_collected_fees(&self, token: AccountId) -> U128 {
        U128(self.collected_fees.get(&token).unwrap_or(0))
    }

    pub fn get_treasury(&self) -> AccountId {
        self.treasury.clone()
    }

    pub fn is_market_authorized(&self, market: AccountId) -> bool {
        self.authorized_markets.get(&market).unwrap_or(false)
    }

    fn assert_owner(&self) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only owner can call this method"
        );
    }

    fn assert_authorized_market(&self) {
        let caller = env::predecessor_account_id();
        assert!(
            self.authorized_markets.get(&caller).unwrap_or(false),
            "Only authorized markets can call this method"
        );
    }
}

#[near]
impl FungibleTokenReceiver for FeeCollector {
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> U128 {
        let token = env::predecessor_account_id();
        
        if msg == "fee" {
            let current = self.collected_fees.get(&token).unwrap_or(0);
            self.collected_fees.insert(&token, &(current + amount.0));
            env::log_str(&format!(
                "Received fee: {} of token {} from {}",
                amount.0, token, sender_id
            ));
            U128(0)
        } else {
            amount
        }
    }
}