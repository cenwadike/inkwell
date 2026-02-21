# stylus-inkwell CLI 🧪

> Forensic ink profiler for Arbitrum Stylus smart contracts.  
> Static analysis + runtime instrumentation. Detects Dry Nib bugs. Generates CI-ready JSON.

---

## Installation


### From Cargo

```bash
cargo install stylus-inkwell 
```

### From Source

```bash
git clone https://github.com/cenwadike/inkwell
cd inkwell/cli
cargo build --release
```

Binary: `target/release/stylus-inkwell`

Add to PATH:

```bash
export PATH="$PATH:$(pwd)/target/release"
```

### Requirements

- Rust 2024 edition (`rustup update stable`)
- For `--profile` mode: a running Arbitrum Stylus node (local or Sepolia)
- For `sol!` macro expansion: `cargo +nightly` + `cargo-expand`

---

## Commands

```
stylus-inkwell <COMMAND>

Commands:
  dip         Static analysis + optional on-chain profiling  [alias: d]
  instrument  Inject runtime ink probes into contract source  [alias: i]
```

---

## `dip` — Analyze Ink Consumption

Parses your Stylus contract via `syn`, walks the AST, estimates ink per operation, detects Dry Nib overcharges, and produces terminal output + `ink-report.json`.

### Usage

```bash
stylus-inkwell dip <FILE> [OPTIONS]
```

### Arguments

| Argument | Description |
|---|---|
| `FILE` | Path to contract source (e.g. `src/lib.rs`) |

### Options

| Flag | Default | Description |
|---|---|---|
| `-f, --function <NAME>` | *(all)* | Analyze a single function by name |
| `-o, --output <FORMAT>` | `compact` | Output format: `compact`, `detailed`, `json` |
| `--threshold <INK>` | `100000` | Min ink to highlight in compact view |
| `--no-color` | false | Disable ANSI colors (for CI logs) |
| `-p, --profile` | false | Enable on-chain runtime profiling |
| `--rpc-url <URL>` | `http://localhost:8547` | RPC endpoint for profiling |
| `--private-key <HEX>` | *(required for --profile)* | Deployer private key |
| `--chain-id <ID>` | `1337` | Chain ID for transactions |
| `--calldata <HEX>` | *(optional)* | 0x-prefixed calldata for profiling tx |
| `--value <WEI>` | `0` | Value to send with profiling tx |
| `--instrumented-output <PATH>` | `instrumented_contract.rs` | Where to save instrumented source |

### Examples

```bash
# Analyze all public functions — compact terminal view
stylus-inkwell dip src/lib.rs

# Analyze one function
stylus-inkwell dip src/lib.rs --function create_market

# Full JSON report (pipe to file or CI artifact)
stylus-inkwell dip src/lib.rs --output json > report.json

# Detailed with category breakdown table
stylus-inkwell dip src/lib.rs --output detailed

# No color for CI/CD logs
stylus-inkwell dip src/lib.rs --no-color

# On-chain profiling (local Stylus devnet)
stylus-inkwell dip src/lib.rs \
  --profile \
  --rpc-url http://localhost:8547 \
  --private-key 0xac0974bec... \
  --calldata 0x1249c58b

# On-chain profiling (Arbitrum Sepolia)
stylus-inkwell dip src/lib.rs \
  --profile \
  --rpc-url https://sepolia-rollup.arbitrum.io/rpc \
  --private-key 0xYOUR_KEY \
  --chain-id 421614 \
  --calldata 0xCALLDATA
```

---

## `instrument` — Inject Runtime Probes

Rewrites your contract source, wrapping expensive operations with ink measurement probes. When compiled with `--features ink-profiling`, the contract records real ink values at runtime and generates a human-readable report via `get_ink_report()`.

### Usage

```bash
stylus-inkwell instrument <FILE> [OPTIONS]
```

### Options

| Flag | Default | Description |
|---|---|---|
| `-o, --output <PATH>` | `instrumented_contract.rs` | Output path for instrumented source |
| `--no-color` | false | Disable colored output |

### Example

```bash
# Instrument the contract
stylus-inkwell instrument src/lib.rs --output src/lib_instrumented.rs

# Build with ink-profiling feature
cargo build --release \
  --target wasm32-unknown-unknown \
  --features ink-profiling

# Deploy and call get_ink_report() to see runtime measurements
```

The instrumented contract adds a `__ink_profiling` module containing:
- `InkTracker` — thread-safe singleton recording before/after ink values per probe
- `check_dry_nib` — detects buffer allocation waste at runtime
- `dump_report()` — human-readable ink usage summary
- `probe_before / probe_after / probe_after_with_size` — inline probe functions

---

## Output Files

After every `dip` run, Inkwell writes two files:

### `ink-report.json` (project root)

Standardized, machine-readable analysis artifact. Drop it into [JSON Hero](https://jsonhero.io) for a visual audit, or consume it in CI/CD.

```json
{
  "contract_name": "Contract",
  "file": "swap/src/lib.rs",
  "functions": {
    "create_market": {
      "name": "create_market",
      "total_ink": 25800000,
      "gas_equivalent": 2580,
      "operations": [
        {
          "line": 168,
          "column": 0,
          "code": "self.indexes.setter(base_token).setter(quote_token).get()",
          "operation": "nested_map_get",
          "entity": "indexes",
          "ink": 1200000,
          "percentage": 4.65,
          "category": "storage_read",
          "severity": "high"
        }
      ],
      "dry_nib_bugs": [
        {
          "line": 168,
          "operation": "indexes (nested mapping)",
          "ink_charged_estimate": 4800000,
          "actual_return_size": 32,
          "buffer_allocated": 64,
          "expected_fair_cost": 1200000,
          "overcharge_estimate": 3600000,
          "severity": "high",
          "mitigation": "Cache outer mapping result before inner .get()"
        }
      ],
      "optimizations": [...],
      "hotspots": [...],
      "categories": {...}
    }
  }
}
```

### `.inkwell/decorations.json`

VS Code decoration data: inline text, gutter icons (flame/bug/lightbulb), hover tooltips (markdown), and code actions. Consumed by the Inkwell VS Code extension.

---

## Reading the Terminal Report

### Compact View

```
🧪 INKWELL STAIN REPORT
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🎯 pub fn create_market(&mut self, ...)
💰 Total: 25,800,000 ink  (≈ 2,580 gas)

════════════════════════════════════════════════════════════
  🐛 DRY NIB BUGS DETECTED - HOST CALL OVERHEAD ISSUES
════════════════════════════════════════════════════════════
  🚨 Bug #1: indexes (nested mapping) at line 168
     │ Operation:          storage_read
     │ Actual return size: 32 bytes
     │ Buffer allocated:   64 bytes
     │ Wastage:            charged for 32 bytes of padding!
     │ Ink charged (est):  4,800,000 ink
     │ Fair cost:          1,200,000 ink
     │ Overcharge:         3,600,000 ink (300.0% overcharge)
     │ Mitigation:         Cache outer mapping result before inner .get()

🔥 Expensive Lines
  Line  168 │ indexes::nested_map_get      4.8M ink  18.6%  ████████████████████
  Line  186 │ markets::map::insert         4.5M ink  17.4%  █████████████████
  Line  195 │ indexes::map::upsert         3.9M ink  15.1%  ██████████████

💡 Optimizations
  Line  168 │ Cache repeated storage read: self.indexes  (savings ~39,600K ink)
```

**Columns:** `Line | Operation | Ink | % of function | Visual bar`

**Icons:**
- 🔥 High-severity line (≥ 2M ink or `severity: high`)
- 🐛 Dry Nib bug detected
- 💡 Caching optimization available
- 🚨 Critical overcharge (> 2× expected fair cost)
- ⚠️ Medium severity

### Detailed View (`--output detailed`)

Adds a category breakdown table after the compact view:

```
📊 CATEGORY SUMMARY

Category        │ Operations │   Total Ink  │   %  │   Avg/Op
───────────────────────────────────────────────────────────────
storage_read    │     12     │  14,400,000  │  55% │  1,200,000 🔥
storage_write   │      6     │   9,000,000  │  34% │  1,500,000 🔥
evm_context     │      2     │     600,000  │   2% │    300,000 🐛
event           │      1     │     350,000  │   1% │    350,000
```

---

## Ink Cost Model

Static analysis uses these per-operation estimates (based on documented Stylus VM behavior):

| Category | Operation | Ink Estimate |
|---|---|---|
| `storage_read` | `map::get` | 1,200,000 |
| `storage_read` | `nested_map_get` | 1,200,000 × depth |
| `storage_read` | `storage::load` | 1,200,000 |
| `storage_write` | `map::insert` | 1,500,000 |
| `storage_write` | `map::upsert` | 1,500,000 + read overhead |
| `evm_context` | `msg::sender()` | 300,000 |
| `evm_context` | `msg::value()` | 350,000 |
| `evm_context` | `block::*` | 250,000 |
| `event` | `evm::log()` | 350,000 |
| `external_call` | `.call()` | 2,500,000 |
| `storage` overhead | field access + load | +2,400,000 per storage op |

Gas equivalent: `total_ink / 10,000`

For precise measurement, use `--profile` mode to capture real ink values via `hostio::ink_left()`.

---

## Dry Nib Bug Detection

Inkwell flags operations as Dry Nib candidates when:

1. **Nested get depth ≥ 2** — e.g. `self.indexes.setter(a).setter(b).get()` double-charges buffer overhead
2. **Known expensive fields** — `balances`, `allowances`, and similar ERC-20 mapping patterns
3. **Ink ≥ 3M** — absolute threshold for single-operation overcharge

For each detected bug, Inkwell reports:

```
ink_charged_estimate   — what Stylus likely charged
actual_return_size     — bytes actually returned
buffer_allocated       — bytes Stylus allocated
expected_fair_cost     — what a fair charge would be
overcharge_estimate    — the difference
severity               — high (> 2M overcharge) or medium
mitigation             — concrete fix suggestion
```

---

## CI/CD Integration

```yaml
# .github/workflows/ink-audit.yml
- name: Run Inkwell
  run: |
    stylus-inkwell dip src/lib.rs --output json --no-color
    
- name: Upload ink report
  uses: actions/upload-artifact@v3
  with:
    name: ink-report
    path: ink-report.json
```

To gate on ink budget (example with `jq`):

```bash
TOTAL_INK=$(jq '[.functions[].total_ink] | add' ink-report.json)
MAX_BUDGET=50000000

if [ "$TOTAL_INK" -gt "$MAX_BUDGET" ]; then
  echo "❌ Ink budget exceeded: $TOTAL_INK > $MAX_BUDGET"
  exit 1
fi
```

---

## Supported Contract Patterns

| Pattern | Supported |
|---|---|
| `#[public]` impl blocks | ✅ |
| `#[external]` impl blocks | ✅ |
| Public methods with `&self`/`&mut self` | ✅ |
| `sol_storage!` macro | ✅ (via `cargo expand`) |
| `sol_interface!` / `sol!` macros | ✅ (via `cargo expand`) |
| `#[entrypoint]` | ✅ |
| Selector-based dispatch (router pattern) | ✅ (heuristic) |

For macro-heavy contracts, Inkwell automatically attempts `cargo +nightly expand` and falls back to the original source on failure.

---

## Troubleshooting

**`No public/external functions detected`**

Your contract uses `sol!` dispatch. Ensure `cargo +nightly` and `cargo-expand` are installed:

```bash
rustup toolchain install nightly
cargo install cargo-expand
```

**`cargo expand failed`**

Check that your contract builds cleanly first:

```bash
cargo build --release --target wasm32-unknown-unknown
```

**`--profile` mode: `No contract address — Stylus activation may be required`**

After deployment, activate the contract:

```bash
cargo stylus activate --address <DEPLOYED_ADDRESS> --endpoint <RPC_URL>
```

**Ink estimates seem off**

Static analysis uses fixed cost estimates. For precise numbers, use `instrument` + `--profile` mode against a live Stylus node.

---

## Architecture

```
main.rs           CLI parsing (clap), dispatches to run_analysis_mode / run_profiling_mode
  │
  ├── analyzer.rs
  │     ContractVisitor (syn::Visit)
  │       ├── visit_item_impl  → detect public/external impl blocks
  │       ├── analyze_function → walk statements, collect Operations
  │       ├── analyze_expr     → detect reads/writes/host-calls recursively
  │       ├── detect_dry_nib_bugs → buffer overcharge detection
  │       ├── detect_optimizations → repeated-read caching
  │       └── calculate_categories → per-category aggregation
  │
  ├── instrumentor.rs
  │     Instrumentor (syn::VisitMut)
  │       ├── visit_item_impl_mut → find instrumentation targets
  │       ├── visit_block_mut     → rewrite statements with probe calls
  │       └── generate_instrumented_code → append __ink_profiling module
  │
  ├── reporter.rs
  │     Reporter
  │       ├── print_compact / print_detailed / print_json
  │       ├── print_dry_nib_bugs
  │       └── generate_vscode_decorations
  │
  └── types.rs
        ContractAnalysis, FunctionAnalysis, Operation,
        DryNibBug, Optimization, Hotspot,
        VsCodeDecorations, Decorations, ...
```

---

## License

Apache-2.0 — see [LICENSE](../LICENSE).