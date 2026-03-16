# Empty Template

This project was generated from Acton's `empty` template. It gives you the
smallest possible project layout with a contract stub, a test stub, and a
deployment script you can adapt to your own contract.

## What Is Included

- `contracts/contract.tolk` contains a minimal contract entrypoint.
- `tests/contract.test.tolk` is a starter test file.
- `scripts/deploy.tolk` is a deployment script template. Uncomment and adapt it
  after you define your storage and wrapper code.

## Build

```bash
acton build
```

## Test

```bash
acton test
```

## Deploy To Testnet

1. Implement your contract in `contracts/contract.tolk`.
2. Add or update a wrapper in `tests/wrappers/`.
3. Uncomment and adapt `scripts/deploy.tolk` so it deploys your contract.
4. Create a local wallet named `deployer` and fund it on testnet:

```bash
acton wallet new --name deployer --local --airdrop
```

5. Run the deployment script against testnet:

```bash
acton script scripts/deploy.tolk --broadcast --net testnet
```

The generated `Acton.toml` also includes shortcut scripts:

```bash
acton run deploy-emulation
acton run deploy-testnet
```

If you hit rate limits while talking to Toncenter, set `TONCENTER_API_KEY` in
`.env`.

## Documentation

- Quickstart: https://i582.github.io/acton/docs/quickstart
- Testing: https://i582.github.io/acton/docs/commands/test
- Scripts and deployment: https://i582.github.io/acton/docs/commands/script
- Wallets: https://i582.github.io/acton/docs/commands/wallet
