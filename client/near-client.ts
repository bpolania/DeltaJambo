import { connect, keyStores, Contract, WalletConnection, utils } from 'near-api-js';
import { FunctionCallOptions } from 'near-api-js/lib/account';
import BN from 'bn.js';

// Configuration
const config = {
  networkId: 'mainnet',
  keyStore: new keyStores.BrowserLocalStorageKeyStore(),
  nodeUrl: 'https://rpc.mainnet.near.org',
  walletUrl: 'https://wallet.mainnet.near.org',
  helperUrl: 'https://helper.mainnet.near.org',
  explorerUrl: 'https://explorer.mainnet.near.org',
};

// Types
export interface MarketParams {
  underlying: string;
  quote: string;
  maturity: string;
  strike_k: string;
  lower_bound_l: string;
  upper_bound_u: string;
  mint_fee_bps: number;
  settle_fee_bps: number;
  redeem_fee_bps: number;
}

export interface MarketInfo {
  market_id: string;
  long_token: string;
  short_token: string;
  params: MarketParams;
  created_at: string;
  creator: string;
}

export interface MarketState {
  is_settled: boolean;
  settlement_price?: string;
  settlement_factor?: string;
  total_collateral: string;
  long_token_supply: string;
  short_token_supply: string;
  paused_mint: boolean;
  paused_settle: boolean;
}

export interface PriceData {
  price: string;
  timestamp: string;
  decimals: number;
}

// Forward Factory Client
export class ForwardFactoryClient {
  private connection: any;
  private wallet: WalletConnection;
  private contract: any;

  constructor(
    private factoryAccountId: string,
    private nearConfig = config
  ) {}

  async init() {
    this.connection = await connect(this.nearConfig);
    this.wallet = new WalletConnection(this.connection, 'forward-markets');
    
    this.contract = new Contract(
      this.wallet.account(),
      this.factoryAccountId,
      {
        viewMethods: [
          'get_market',
          'get_market_by_params',
          'get_markets_by_creator',
          'get_all_markets',
          'get_market_count',
        ],
        changeMethods: [
          'deploy_market',
          'set_paused',
          'update_oracle',
          'update_fee_collector',
          'update_guardian',
        ],
      }
    );
  }

  async deployMarket(params: MarketParams): Promise<void> {
    const deposit = utils.format.parseNearAmount('5'); // 5 NEAR for deployment
    
    await this.contract.deploy_market({
      args: { params },
      gas: new BN('300000000000000'),
      amount: deposit,
    });
  }

  async getMarket(marketKey: string): Promise<MarketInfo | null> {
    return await this.contract.get_market({ market_key: marketKey });
  }

  async getMarketByParams(params: MarketParams): Promise<MarketInfo | null> {
    return await this.contract.get_market_by_params({ params });
  }

  async getMarketsByCreator(creator: string): Promise<MarketInfo[]> {
    return await this.contract.get_markets_by_creator({ creator });
  }

  async getAllMarkets(fromIndex = 0, limit = 100): Promise<MarketInfo[]> {
    return await this.contract.get_all_markets({
      from_index: fromIndex.toString(),
      limit: limit.toString(),
    });
  }

  async getMarketCount(): Promise<number> {
    const count = await this.contract.get_market_count();
    return parseInt(count);
  }
}

// Forward Market Client
export class ForwardMarketClient {
  private connection: any;
  private wallet: WalletConnection;
  private contract: any;

  constructor(
    private marketAccountId: string,
    private nearConfig = config
  ) {}

  async init() {
    this.connection = await connect(this.nearConfig);
    this.wallet = new WalletConnection(this.connection, 'forward-markets');
    
    this.contract = new Contract(
      this.wallet.account(),
      this.marketAccountId,
      {
        viewMethods: [
          'get_market_params',
          'get_market_state',
          'get_user_deposit',
          'preview_settlement',
        ],
        changeMethods: [
          'create_position',
          'redeem',
          'settle',
          'set_paused',
        ],
      }
    );
  }

  async createPosition(amount: string): Promise<void> {
    await this.contract.create_position({
      args: { amount },
      gas: new BN('100000000000000'),
    });
  }

  async redeem(longAmount: string, shortAmount: string): Promise<void> {
    await this.contract.redeem({
      args: {
        long_amount: longAmount,
        short_amount: shortAmount,
      },
      gas: new BN('100000000000000'),
    });
  }

  async settle(): Promise<void> {
    await this.contract.settle({
      gas: new BN('100000000000000'),
    });
  }

  async getMarketParams(): Promise<MarketParams> {
    return await this.contract.get_market_params();
  }

  async getMarketState(): Promise<MarketState> {
    return await this.contract.get_market_state();
  }

  async getUserDeposit(account: string): Promise<string> {
    return await this.contract.get_user_deposit({ account });
  }

  async previewSettlement(hypotheticalPrice: string): Promise<[string, string]> {
    return await this.contract.preview_settlement({
      hypothetical_price: hypotheticalPrice,
    });
  }
}

// NEP-141 Token Client (for LONG/SHORT tokens)
export class NEP141TokenClient {
  private connection: any;
  private wallet: WalletConnection;
  private contract: any;

  constructor(
    private tokenAccountId: string,
    private nearConfig = config
  ) {}

  async init() {
    this.connection = await connect(this.nearConfig);
    this.wallet = new WalletConnection(this.connection, 'forward-markets');
    
    this.contract = new Contract(
      this.wallet.account(),
      this.tokenAccountId,
      {
        viewMethods: [
          'ft_balance_of',
          'ft_total_supply',
          'ft_metadata',
        ],
        changeMethods: [
          'ft_transfer',
          'ft_transfer_call',
          'storage_deposit',
        ],
      }
    );
  }

  async getBalance(accountId: string): Promise<string> {
    return await this.contract.ft_balance_of({ account_id: accountId });
  }

  async getTotalSupply(): Promise<string> {
    return await this.contract.ft_total_supply();
  }

  async getMetadata(): Promise<any> {
    return await this.contract.ft_metadata();
  }

  async transfer(receiverId: string, amount: string, memo?: string): Promise<void> {
    await this.contract.ft_transfer({
      args: {
        receiver_id: receiverId,
        amount,
        memo,
      },
      gas: new BN('30000000000000'),
      amount: '1', // 1 yoctoNEAR for security
    });
  }

  async storageDeposit(accountId?: string): Promise<void> {
    const deposit = utils.format.parseNearAmount('0.00125'); // Storage deposit
    
    await this.contract.storage_deposit({
      args: {
        account_id: accountId,
      },
      amount: deposit,
    });
  }
}

// Oracle Router Client (Updated for Rhea Finance)
export class OracleRouterClient {
  private connection: any;
  private wallet: WalletConnection;
  private contract: any;

  constructor(
    private oracleAccountId: string,
    private nearConfig = config
  ) {}

  async init() {
    this.connection = await connect(this.nearConfig);
    this.wallet = new WalletConnection(this.connection, 'forward-markets');
    
    this.contract = new Contract(
      this.wallet.account(),
      this.oracleAccountId,
      {
        viewMethods: [
          'get_price',
          'get_oracle_config',
        ],
        changeMethods: [
          'configure_oracle',
          'fetch_price',
          'fetch_and_cache_price',
          'set_paused',
        ],
      }
    );
  }

  async getPrice(underlying: string, quote: string): Promise<PriceData | null> {
    return await this.contract.get_price({ underlying, quote });
  }

  async fetchPrice(underlying: string, quote: string): Promise<void> {
    await this.contract.fetch_price({
      args: { underlying, quote },
      gas: new BN('50000000000000'),
    });
  }

  async fetchAndCachePrice(underlying: string, quote: string): Promise<void> {
    await this.contract.fetch_and_cache_price({
      args: { underlying, quote },
      gas: new BN('50000000000000'),
    });
  }

  async configureOracle(
    underlying: string,
    quote: string,
    poolId: number,
    twapWindow: number = 300, // 5 minutes default
    useStablePool: boolean = false
  ): Promise<void> {
    await this.contract.configure_oracle({
      args: {
        underlying,
        quote,
        config: {
          rhea_pool_id: poolId,
          twap_window: twapWindow,
          max_staleness: 600, // 10 minutes
          max_deviation_bps: 500, // 5%
          use_stable_pool: useStablePool,
        },
      },
      gas: new BN('30000000000000'),
    });
  }
}

// Example usage
export async function example() {
  // Initialize factory client
  const factoryClient = new ForwardFactoryClient('factory.deltajambo.near');
  await factoryClient.init();

  // Deploy a new market
  const params: MarketParams = {
    underlying: 'wrap.near',
    quote: 'usdc.near',
    maturity: '1735689600', // Jan 1, 2025
    strike_k: '50000000000000000000000000', // 50 (24 decimals)
    lower_bound_l: '30000000000000000000000000', // 30
    upper_bound_u: '70000000000000000000000000', // 70
    mint_fee_bps: 30,
    settle_fee_bps: 50,
    redeem_fee_bps: 20,
  };

  await factoryClient.deployMarket(params);

  // Get deployed market
  const marketInfo = await factoryClient.getMarketByParams(params);
  if (!marketInfo) {
    console.error('Market not found');
    return;
  }

  // Initialize market client
  const marketClient = new ForwardMarketClient(marketInfo.market_id);
  await marketClient.init();

  // Create position (mint LONG and SHORT tokens)
  const depositAmount = '1000000000000000000000000'; // 1 token with 24 decimals
  await marketClient.createPosition(depositAmount);

  // Check market state
  const state = await marketClient.getMarketState();
  console.log('Market state:', state);

  // Initialize token clients
  const longTokenClient = new NEP141TokenClient(marketInfo.long_token);
  await longTokenClient.init();

  const shortTokenClient = new NEP141TokenClient(marketInfo.short_token);
  await shortTokenClient.init();

  // Check balances
  const accountId = 'user.near';
  const longBalance = await longTokenClient.getBalance(accountId);
  const shortBalance = await shortTokenClient.getBalance(accountId);
  
  console.log('LONG balance:', longBalance);
  console.log('SHORT balance:', shortBalance);

  // Preview settlement at different prices
  const [longValue1, shortValue1] = await marketClient.previewSettlement('40000000000000000000000000');
  console.log('At price 40: LONG =', longValue1, 'SHORT =', shortValue1);

  const [longValue2, shortValue2] = await marketClient.previewSettlement('60000000000000000000000000');
  console.log('At price 60: LONG =', longValue2, 'SHORT =', shortValue2);
}

// Export default client setup
export default {
  ForwardFactoryClient,
  ForwardMarketClient,
  NEP141TokenClient,
  OracleRouterClient,
};