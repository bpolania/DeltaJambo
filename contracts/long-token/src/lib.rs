use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider, FT_METADATA_SPEC,
};
use near_contract_standards::fungible_token::FungibleToken;
use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::{env, near, AccountId, PanicOnDefault, PromiseOrValue};

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct LongToken {
    token: FungibleToken,
    metadata: FungibleTokenMetadata,
    market: AccountId,
}

#[near]
impl LongToken {
    #[init]
    pub fn new(
        market: AccountId,
        name: String,
        symbol: String,
        decimals: u8,
    ) -> Self {
        assert!(!env::state_exists(), "Already initialized");
        
        let metadata = FungibleTokenMetadata {
            spec: FT_METADATA_SPEC.to_string(),
            name,
            symbol,
            icon: None,
            reference: None,
            reference_hash: None,
            decimals,
        };

        let mut this = Self {
            token: FungibleToken::new(b"t".to_vec()),
            metadata,
            market: market.clone(),
        };
        
        this.token.internal_register_account(&market);
        this
    }

    pub fn mint(&mut self, account_id: AccountId, amount: U128) {
        self.assert_market();
        self.token.internal_deposit(&account_id, amount.into());
    }

    pub fn burn(&mut self, account_id: AccountId, amount: U128) {
        self.assert_market();
        self.token.internal_withdraw(&account_id, amount.into());
    }

    fn assert_market(&self) {
        assert_eq!(
            env::predecessor_account_id(),
            self.market,
            "Only market can mint/burn"
        );
    }
}

#[near]
impl FungibleTokenMetadataProvider for LongToken {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.clone()
    }
}

near_contract_standards::impl_fungible_token_core!(LongToken, token);
near_contract_standards::impl_fungible_token_storage!(LongToken, token);