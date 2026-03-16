# Jetton Template

This project was generated from Acton's `jetton` template. It includes a jetton
minter contract, a jetton wallet contract, wrappers, tests, and a deployment
script that deploys the minter and mints the initial supply.

## What Is Included

- `contracts/jetton-minter-contract.tolk` implements the jetton minter.
- `contracts/jetton-wallet-contract.tolk` implements user jetton wallets.
- `tests/wallet.test.tolk` covers minting, admin updates, content updates, and
  transfers.
- `scripts/deploy.tolk` builds on-chain metadata, deploys the minter, and mints
  the configured supply.

## Build

```bash
acton build
```

## Test

```bash
acton test
```

## Deploy To Testnet

The deploy script expects a wallet named `deployer` and optionally reads these
environment variables from `.env` or your shell:

- `JETTON_NAME`
- `JETTON_DESCRIPTION`
- `JETTON_SYMBOL`
- `JETTON_IMAGE`
- `JETTON_SUPPLY`

1. Create a local wallet and request testnet TON:

```bash
acton wallet new --name deployer --local --airdrop
```

2. Optionally customize jetton metadata and supply in `.env`.
3. Broadcast the deployment to testnet:

```bash
acton script scripts/deploy.tolk --broadcast --net testnet
```

You can also use the generated script aliases:

```bash
acton run deploy-emulation
acton run deploy-testnet
```

If you need higher Toncenter limits for blockchain queries, set
`TONCENTER_API_KEY` in `.env`.

## Documentation

- Quickstart: https://i582.github.io/acton/docs/quickstart
- Testing: https://i582.github.io/acton/docs/commands/test
- Scripts and deployment: https://i582.github.io/acton/docs/commands/script
- Wallets: https://i582.github.io/acton/docs/commands/wallet
