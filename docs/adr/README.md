# Architecture Decision Records

| ADR                                                          | Title                                                             | Status                        |
| ------------------------------------------------------------ | ----------------------------------------------------------------- | ----------------------------- |
| [ADR-001](001-use-i64-for-monetary-amounts.md)               | Use i64 for Monetary Amounts                                      | Accepted                      |
| [ADR-002](002-replace-asset-account-with-holding.md)         | Replace `AssetAccount` with `Holding`                             | Accepted                      |
| [ADR-003](003-cross-context-use-case-orchestration.md)       | Cross-Context Use Case Orchestration via Sequential Service Calls | Accepted — amended by ADR-005 |
| [ADR-004](004-use-cases-inject-services-not-repositories.md) | Use Cases Inject Services, Not Repositories                       | Accepted                      |
| [ADR-005](005-account-details-inject-transaction-service.md) | Inject TransactionService into account_details for Realized P&L   | Accepted — amends ADR-003     |
| [ADR-006](006-unit-of-work.md)                               | Unit of Work Pattern for Cross-Aggregate Atomicity                | Accepted                      |
| [ADR-007](007-e2e-combobox-boundary.md)                      | E2E Test Boundary at HeadlessUI ComboboxField                     | Accepted                      |
| [ADR-008](008-asset-price-provider-chain.md)                 | Asset Price Provider Chain: Stooq + Finnhub-BYOK + Manual         | Accepted                      |
| [ADR-009](009-fx-rate-provider-chain.md)                     | FX Rate Provider Chain: Frankfurter + ECB XML + Manual            | Accepted                      |
| [ADR-010](010-source-qualifier-precedence.md)                | Source-Qualifier Precedence: Manual Overrides External            | Accepted                      |
| [ADR-011](011-byok-api-keys-os-keychain.md)                  | User-Supplied API Keys via OS Keychain (3-Tier Linux Fallback)    | Accepted                      |
