# Counter Template

This project was generated from Acton's `counter` template. It includes a small
counter contract, wrapper helpers, tests, and a ready-to-run deployment script.

## What Is Included

- `contracts/counter.tolk` implements the counter contract.
- `contracts/types.tolk` defines storage and message types.
- `tests/counter.test.tolk` covers increment, reset, and invalid-message flows.
- `scripts/deploy.tolk` deploys the contract with initial counter state.

## Build

```bash
acton build
```

## Test

```bash
acton test
```

## Deploy To Testnet

The deployment script expects a wallet named `deployer`.

1. Create a local wallet and request testnet TON:

```bash
acton wallet new --name deployer --local --airdrop
```

2. Broadcast the deployment to testnet:

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
