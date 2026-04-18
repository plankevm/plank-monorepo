# Multisig Wallet

A wallet that requires multiple owners to approve a transaction before it can be executed. Owners submit and confirm transactions, and once a threshold of approvals is reached, anyone can trigger execution.

Beyond previous examples, this showcases:

- Signature verification using the `ecrecover` precompile
- Forwarding arbitrary calls with `call`
- Multi-party approval logic with threshold counting
- More complex storage layouts: tracking owners, confirmations, and transaction queues
