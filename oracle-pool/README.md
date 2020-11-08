# Oracle pool program

An oracle pool protocol for the Token program on the Solana blockchain inspired by Synthetix.

```
type TokenAccount = Pubkey;
type Price = Pubkey;
type Mint = Pubkey;

enum MintOrPrice {
    Mint(Mint),
    Price(Price),
}

/// Oracle-pool keeps track of three states:
/// price oracle => Mint address OR another price oracle
oracles: HashMap<Price, MintOrPrice>
/// physical settlement token map to account that holds the token
settlement_pools: HashMap<Mint, TokenAccount>
/// token account
pool_shares: TokenAccount

/// Add a new settlement pool with a price feed
add_settlement_pool(mint: Mint, price: Price)

/// Supports 1 swap method that can route between the inputs and outputs
swap(settlement_tokens: Token) -> Pool Shares at present value
swap(pool_shares) -> Synthetic price tokens at present value
swap(synthetic_tokens) -> Settlement tokens

/// Price is a vector of

(Slot, ID, Bid, Ask)
```

Full documentation will be made available at https://spl.solana.com in the future

Web3 bindings are available in the `./js` directory.
