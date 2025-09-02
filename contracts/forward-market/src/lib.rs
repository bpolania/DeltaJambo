use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{env, ext_contract, near, require, AccountId, Balance, Gas, PanicOnDefault, Promise, PromiseResult};

const TGAS: u64 = 1_000_000_000_000;
const FT_TRANSFER_GAS: Gas = Gas::from_tgas(10);
const DEPLOY_GAS: Gas = Gas::from_tgas(50);
const CALLBACK_GAS: Gas = Gas::from_tgas(10);

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

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone)]
#[serde(crate = "near_sdk::serde")]
pub struct MarketState {
    pub is_settled: bool,
    pub settlement_price: Option<U128>,
    pub settlement_factor: Option<U128>,
    pub total_collateral: Balance,
    pub long_token_supply: Balance,
    pub short_token_supply: Balance,
    pub paused_mint: bool,
    pub paused_settle: bool,
}

#[ext_contract(ext_ft)]
trait FungibleToken {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> Promise;
    fn ft_balance_of(&self, account_id: AccountId) -> U128;
}

#[ext_contract(ext_token)]
trait ClaimToken {
    fn mint(&mut self, account_id: AccountId, amount: U128);
    fn burn(&mut self, account_id: AccountId, amount: U128);
}

#[ext_contract(ext_oracle)]
trait OracleRouter {
    fn get_price(&self, underlying: AccountId, quote: AccountId) -> Option<PriceData>;
}

#[ext_contract(ext_fee_collector)]
trait FeeCollector {
    fn record_fee(&mut self, token: AccountId, amount: Balance);
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct PriceData {
    pub price: U128,
    pub timestamp: u64,
    pub decimals: u8,
}

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct ForwardMarket {
    params: MarketParams,
    state: MarketState,
    long_token: AccountId,
    short_token: AccountId,
    oracle: AccountId,
    fee_collector: AccountId,
    owner: AccountId,
    guardian: AccountId,
    user_deposits: UnorderedMap<AccountId, Balance>,
    pending_actions: UnorderedMap<String, PendingAction>,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct PendingAction {
    pub account: AccountId,
    pub amount: Balance,
    pub action_type: ActionType,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub enum ActionType {
    Mint,
    Redeem,
}

#[near]
impl ForwardMarket {
    #[init]
    pub fn new(
        params: MarketParams,
        long_token: AccountId,
        short_token: AccountId,
        oracle: AccountId,
        fee_collector: AccountId,
        owner: AccountId,
        guardian: AccountId,
    ) -> Self {
        assert!(!env::state_exists(), "Already initialized");
        require!(params.upper_bound_u > params.lower_bound_l, "Invalid bounds");
        require!(params.strike_k >= params.lower_bound_l, "Strike below lower bound");
        require!(params.strike_k <= params.upper_bound_u, "Strike above upper bound");
        require!(params.maturity > env::block_timestamp(), "Maturity in past");
        
        Self {
            params,
            state: MarketState {
                is_settled: false,
                settlement_price: None,
                settlement_factor: None,
                total_collateral: 0,
                long_token_supply: 0,
                short_token_supply: 0,
                paused_mint: false,
                paused_settle: false,
            },
            long_token,
            short_token,
            oracle,
            fee_collector,
            owner,
            guardian,
            user_deposits: UnorderedMap::new(b"d"),
            pending_actions: UnorderedMap::new(b"p"),
        }
    }

    pub fn create_position(&mut self, amount: U128) -> Promise {
        require!(!self.state.paused_mint, "Minting is paused");
        require!(!self.state.is_settled, "Market is settled");
        require!(amount.0 > 0, "Amount must be positive");
        
        let account = env::predecessor_account_id();
        let action_id = format!("mint_{}", env::block_index());
        
        self.pending_actions.insert(&action_id, &PendingAction {
            account: account.clone(),
            amount: amount.0,
            action_type: ActionType::Mint,
        });
        
        ext_ft::ext(self.params.quote.clone())
            .with_static_gas(FT_TRANSFER_GAS)
            .ft_transfer_call(
                env::current_account_id(),
                amount,
                None,
                action_id,
            )
    }

    pub fn redeem(&mut self, long_amount: U128, short_amount: U128) -> Promise {
        require!(self.state.is_settled, "Market not settled");
        require!(long_amount.0 > 0 || short_amount.0 > 0, "No tokens to redeem");
        
        let account = env::predecessor_account_id();
        let settlement_factor = self.state.settlement_factor.expect("Settlement factor not set");
        
        let long_payout = self.calculate_payout(long_amount.0, settlement_factor.0, true);
        let short_payout = self.calculate_payout(short_amount.0, settlement_factor.0, false);
        let total_payout = long_payout + short_payout;
        
        let fee = (total_payout * self.params.redeem_fee_bps as u128) / 10000;
        let net_payout = total_payout - fee;
        
        require!(net_payout <= self.state.total_collateral, "Insufficient collateral");
        
        self.state.total_collateral -= total_payout;
        
        if long_amount.0 > 0 {
            ext_token::ext(self.long_token.clone())
                .with_static_gas(FT_TRANSFER_GAS)
                .burn(account.clone(), long_amount);
        }
        
        if short_amount.0 > 0 {
            ext_token::ext(self.short_token.clone())
                .with_static_gas(FT_TRANSFER_GAS)
                .burn(account.clone(), short_amount);
        }
        
        if fee > 0 {
            ext_fee_collector::ext(self.fee_collector.clone())
                .with_static_gas(FT_TRANSFER_GAS)
                .record_fee(self.params.quote.clone(), fee);
        }
        
        ext_ft::ext(self.params.quote.clone())
            .with_static_gas(FT_TRANSFER_GAS)
            .ft_transfer(
                account,
                U128(net_payout),
                Some("Redemption payout".to_string()),
            )
    }

    pub fn settle(&mut self) -> Promise {
        require!(!self.state.paused_settle, "Settlement is paused");
        require!(!self.state.is_settled, "Already settled");
        require!(env::block_timestamp() >= self.params.maturity, "Not mature yet");
        
        ext_oracle::ext(self.oracle.clone())
            .with_static_gas(Gas::from_tgas(10))
            .get_price(self.params.underlying.clone(), self.params.quote.clone())
            .then(
                Self::ext(env::current_account_id())
                    .with_static_gas(CALLBACK_GAS)
                    .on_price_received()
            )
    }

    #[private]
    pub fn on_price_received(&mut self) -> bool {
        match env::promise_result(0) {
            PromiseResult::Successful(value) => {
                if let Ok(price_data) = near_sdk::serde_json::from_slice::<Option<PriceData>>(&value) {
                    if let Some(price) = price_data {
                        self.finalize_settlement(price.price);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn finalize_settlement(&mut self, price: U128) {
        let settlement_factor = self.calculate_settlement_factor(price.0);
        
        let fee = (self.state.total_collateral * self.params.settle_fee_bps as u128) / 10000;
        self.state.total_collateral -= fee;
        
        self.state.is_settled = true;
        self.state.settlement_price = Some(price);
        self.state.settlement_factor = Some(U128(settlement_factor));
        
        if fee > 0 {
            ext_fee_collector::ext(self.fee_collector.clone())
                .with_static_gas(FT_TRANSFER_GAS)
                .record_fee(self.params.quote.clone(), fee);
        }
        
        env::log_str(&format!(
            "Market settled: price={}, factor={}",
            price.0, settlement_factor
        ));
    }

    fn calculate_settlement_factor(&self, price: u128) -> u128 {
        let l = self.params.lower_bound_l.0;
        let u = self.params.upper_bound_u.0;
        
        if price <= l {
            0
        } else if price >= u {
            10_u128.pow(24)
        } else {
            ((price - l) * 10_u128.pow(24)) / (u - l)
        }
    }

    fn calculate_payout(&self, amount: u128, settlement_factor: u128, is_long: bool) -> u128 {
        if is_long {
            (amount * settlement_factor) / 10_u128.pow(24)
        } else {
            (amount * (10_u128.pow(24) - settlement_factor)) / 10_u128.pow(24)
        }
    }

    pub fn preview_settlement(&self, hypothetical_price: U128) -> (U128, U128) {
        let factor = self.calculate_settlement_factor(hypothetical_price.0);
        let long_value = (10_u128.pow(24) * factor) / 10_u128.pow(24);
        let short_value = 10_u128.pow(24) - long_value;
        (U128(long_value), U128(short_value))
    }

    pub fn set_paused(&mut self, pause_mint: bool, pause_settle: bool) {
        require!(
            env::predecessor_account_id() == self.guardian || env::predecessor_account_id() == self.owner,
            "Not authorized"
        );
        self.state.paused_mint = pause_mint;
        self.state.paused_settle = pause_settle;
    }

    pub fn get_market_params(&self) -> MarketParams {
        self.params.clone()
    }

    pub fn get_market_state(&self) -> MarketState {
        self.state.clone()
    }

    pub fn get_user_deposit(&self, account: AccountId) -> U128 {
        U128(self.user_deposits.get(&account).unwrap_or(0))
    }
}

#[near]
impl FungibleTokenReceiver for ForwardMarket {
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> U128 {
        require!(env::predecessor_account_id() == self.params.quote, "Wrong token");
        
        if let Some(action) = self.pending_actions.get(&msg) {
            if action.account == sender_id && action.amount == amount.0 {
                match action.action_type {
                    ActionType::Mint => {
                        let fee = (amount.0 * self.params.mint_fee_bps as u128) / 10000;
                        let net_amount = amount.0 - fee;
                        
                        self.state.total_collateral += net_amount;
                        self.state.long_token_supply += net_amount;
                        self.state.short_token_supply += net_amount;
                        
                        let current = self.user_deposits.get(&sender_id).unwrap_or(0);
                        self.user_deposits.insert(&sender_id, &(current + net_amount));
                        
                        ext_token::ext(self.long_token.clone())
                            .with_static_gas(FT_TRANSFER_GAS)
                            .mint(sender_id.clone(), U128(net_amount));
                        
                        ext_token::ext(self.short_token.clone())
                            .with_static_gas(FT_TRANSFER_GAS)
                            .mint(sender_id.clone(), U128(net_amount));
                        
                        if fee > 0 {
                            ext_fee_collector::ext(self.fee_collector.clone())
                                .with_static_gas(FT_TRANSFER_GAS)
                                .record_fee(self.params.quote.clone(), fee);
                        }
                        
                        self.pending_actions.remove(&msg);
                        U128(0)
                    }
                    _ => amount,
                }
            } else {
                amount
            }
        } else {
            amount
        }
    }
}