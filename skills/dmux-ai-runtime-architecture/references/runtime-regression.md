# Runtime Regression

Use this when verifying the current dmux AI hook-only runtime path.

## Fast path

- `swift test --filter AIRuntimeIngressHookEventTests`
- `swift test --filter AIRuntimeIngressSocketTests`
- `swift test --filter AISessionStoreTests`

Manual in-terminal fake hook commands were removed. Runtime regression now covers only the real hook/socket path and automated tests.
