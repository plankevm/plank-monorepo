# Simple Vault (ERC4626)

A tokenized vault where users deposit an underlying token and receive shares proportional to the vault's total holdings. Shares can be redeemed to withdraw the underlying token.

Beyond previous examples, this showcases:

- Cross-contract calls: interacting with an external ERC20 token via `call` and `staticcall`
- Share math: computing deposit/withdrawal amounts using proportion calculations
- Comptime: precomputing constants like precision factors and selector values at compile time with zero runtime cost
