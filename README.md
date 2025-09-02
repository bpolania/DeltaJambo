# DeltaJambo - NEAR Forward Markets Protocol

A non-leveraged, cash-settled forward product for meme coins on NEAR Protocol. The system implements a "capped linear forward market" primitive where users can take long or short positions that settle based on the underlying asset price at maturity.

## Architecture

### Core Contracts

1. **ForwardFactory** - Deploys and manages forward markets
2. **ForwardMarket** - Individual market for a specific underlying/quote pair with defined parameters
3. **LongToken** - NEP-141 token representing long positions
4. **ShortToken** - NEP-141 token representing short positions
5. **FeeCollector** - Collects and manages protocol fees
6. **OracleRouter** - Integrates with Ref Finance for price feeds

### Key Features

- **No Leverage**: Fully collateralized positions with no liquidation risk
- **Bounded Payoffs**: Linear payoffs capped between lower (L) and upper (U) bounds
- **Cash Settlement**: All settlements in quote stablecoin
- **Equal Value Minting**: LONG and SHORT tokens always minted in equal amounts
- **Fee Structure**: Configurable mint, settlement, and redemption fees

## Settlement Mechanics

The settlement factor `p` is calculated as:
```
p = clamp((S_T − L)/(U − L), 0, 1)
```

Where:
- `S_T` is the settlement price at maturity
- `L` is the lower bound
- `U` is the upper bound
- LONG tokens redeem for `p` share of the pool
- SHORT tokens redeem for `(1 - p)` share of the pool

## Building

```bash
# Install Rust and NEAR tools
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown

# Build contracts
./build.sh

# Run tests
cargo test
```

## Deployment

```bash
# Deploy factory
near deploy factory.testnet res/forward-factory.wasm new '{
  "owner": "owner.testnet",
  "oracle": "oracle.testnet",
  "fee_collector": "fees.testnet",
  "guardian": "guardian.testnet"
}'

# Set contract codes
near call factory.testnet set_contract_codes '{
  "market_code": [...],
  "long_token_code": [...],
  "short_token_code": [...]
}' --accountId owner.testnet

# Deploy a market
near call factory.testnet deploy_market '{
  "params": {
    "underlying": "wrap.near",
    "quote": "usdc.near",
    "maturity": "1735689600",
    "strike_k": "50000000000000000000000000",
    "lower_bound_l": "30000000000000000000000000",
    "upper_bound_u": "70000000000000000000000000",
    "mint_fee_bps": 30,
    "settle_fee_bps": 50,
    "redeem_fee_bps": 20
  }
}' --accountId user.testnet --deposit 5
```

## Usage

### Create Position
```bash
# Approve quote token
near call usdc.near ft_transfer_call '{
  "receiver_id": "market.testnet",
  "amount": "1000000000000000000000000",
  "msg": ""
}' --accountId user.testnet --depositYocto 1

# Create position (mints equal LONG and SHORT)
near call market.testnet create_position '{
  "amount": "1000000000000000000000000"
}' --accountId user.testnet
```

### Settle Market
```bash
# After maturity, anyone can trigger settlement
near call market.testnet settle '{}' --accountId anyone.testnet
```

### Redeem Tokens
```bash
# After settlement, redeem tokens for quote currency
near call market.testnet redeem '{
  "long_amount": "500000000000000000000000",
  "short_amount": "500000000000000000000000"
}' --accountId user.testnet
```

## TypeScript Client

```typescript
import { ForwardFactoryClient, ForwardMarketClient } from './client/near-client';

// Initialize factory
const factory = new ForwardFactoryClient('factory.testnet');
await factory.init();

// Deploy market
await factory.deployMarket({
  underlying: 'wrap.near',
  quote: 'usdc.near',
  maturity: '1735689600',
  strike_k: '50000000000000000000000000',
  lower_bound_l: '30000000000000000000000000',
  upper_bound_u: '70000000000000000000000000',
  mint_fee_bps: 30,
  settle_fee_bps: 50,
  redeem_fee_bps: 20,
});

// Get market info
const market = await factory.getMarketByParams(params);

// Initialize market client
const marketClient = new ForwardMarketClient(market.market_id);
await marketClient.init();

// Create position
await marketClient.createPosition('1000000000000000000000000');

// Preview settlement
const [longValue, shortValue] = await marketClient.previewSettlement('45000000000000000000000000');
```

## Security Considerations

- All contracts use NEAR SDK's built-in reentrancy guards
- Pausability implemented for emergency stops
- Guardian role for operational security
- Owner role for parameter updates
- Strict validation of market parameters
- No external dependencies beyond NEAR SDK and Ref Finance

## License

MIT
