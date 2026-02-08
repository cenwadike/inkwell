# 🧪 Inkwell

> **Dip deep into Stylus contract gas analysis**

Inkwell is a powerful profiler and analyzer for [Arbitrum Stylus](https://docs.arbitrum.io/stylus/stylus-gentle-introduction) smart contracts written in Rust. It helps you understand and optimize ink (gas) consumption through static analysis and runtime profiling.

## Features

- **🔬 Static Analysis**: Analyze contracts without deployment
- **🔥 Hotspot Detection**: Identify expensive operations (>1M ink)
- **💡 Optimization Suggestions**: Get actionable recommendations
- **📊 Category Breakdown**: Understand ink distribution by operation type
- **🧬 Code Instrumentation**: Inject profiling probes automatically
- **⚡ Real Profiling**: Measure actual on-chain ink consumption
- **🎨 Rich Output**: Colorized terminal reports with visual indicators

## Installation

```bash
cargo install --path .

export PATH="$HOME/.cargo/bin:$PATH"
```

_reload the terminal_ or `source ~/.zshrc`

Or build from source:

```bash
git clone https://github.com/cenwadike/inkwell
cd inkwell
cargo build --release
```

## Quick Start

### Static Analysis (Recommended First Step)

Analyze your contract without deploying it:

```bash
inkwell dip src/contract.rs
```

With detailed category breakdown:

```bash
inkwell dip src/contract.rs --output detailed
```

Target a specific function:

```bash
inkwell dip src/contract.rs --function transfer
```

### Code Instrumentation

Inject ink tracking probes into your contract:

```bash
inkwell instrument src/contract.rs --output instrumented.rs
```

Then build with profiling enabled:

```bash
cargo build --release --target wasm32-unknown-unknown --features ink-profiling
```

### Runtime Profiling

Deploy and profile your contract on a live chain:

```bash
inkwell dip src/contract.rs \
  --profile \
  --rpc-url http://localhost:8547 \
  --private-key 0xYOUR_PRIVATE_KEY \
  --chain-id 1337 
```

This will:
1. ✅ Instrument your contract
2. ✅ Build WASM binary
3. ✅ Deploy to chain
4. ✅ Execute profiling transaction
5. ✅ Fetch and display real ink consumption

## Command Reference

### `inkwell dip`

The main command for analysis and profiling.

**Aliases:** `d`

**Usage:**
```bash
inkwell dip <FILE> [OPTIONS]
```

**Options:**

| Flag | Description | Default |
|------|-------------|---------|
| `-f, --function <NAME>` | Target specific function | All functions |
| `-o, --output <FORMAT>` | Output format: `compact`, `detailed`, `json` | `compact` |
| `--threshold <INK>` | Ink threshold for highlighting | `100000` |
| `--no-color` | Disable colored output | Colors enabled |
| `-p, --profile` | Enable real on-chain profiling | Static analysis |
| `--rpc-url <URL>` | RPC endpoint for profiling | `http://localhost:8547` |
| `--private-key <KEY>` | Private key (profiling mode) | Required for profiling |
| `--chain-id <ID>` | Chain ID for transactions | `1337` |
| `--calldata <HEX>` | Transaction calldata | None |
| `--value <WEI>` | Transaction value in wei | `0` |
| `--instrumented-output <PATH>` | Save instrumented code | `instrumented_contract.rs` |

### `inkwell instrument`

Instrument contract with profiling probes.

**Aliases:** `i`

**Usage:**
```bash
inkwell instrument <FILE> [OPTIONS]
```

**Options:**

| Flag | Description | Default |
|------|-------------|---------|
| `-o, --output <PATH>` | Output path for instrumented code | `instrumented_contract.rs` |
| `--no-color` | Disable colored output | Colors enabled |

## Output Examples

### Compact Output

```
🧪 INKWELL STAIN REPORT
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🎯 Function: transfer(to: Address, amount: U256)
💰 Total Ink: 5,200,000 (≈ 520 gas)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🔥 HOTSPOTS (Operations > 1M ink)

  Line  42  │  storage_read (get())           1.2M  ████████████  23%
  Line  45  │  storage_write (write())        1.5M  ███████████████  29% 🔥
  Line  48  │  storage_read (get())           1.2M  ████████████  23%

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

💡 OPTIMIZATION OPPORTUNITIES

  Line 45  │ Redundant Storage Read in Write
           │ Separate read into local variable to save ~1.2M ink.
           │
           │ Suggestion:
           │   // let cached = storage.get(key);
           │   // storage.set(key, cached + value);
           │
           │ 💰 Potential savings: ~1200K ink (50% reduction)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### JSON Output

```bash
inkwell dip src/contract.rs --output json > report.json
```

Generates structured JSON with all analysis data, perfect for CI/CD integration.

## How It Works

### Static Analysis

1. **Parse Rust AST**: Uses `syn` to parse contract source
2. **Detect Operations**: Identifies storage reads/writes, events, crypto operations
3. **Estimate Ink**: Assigns ink costs based on operation type
4. **Find Patterns**: Detects anti-patterns like embedded reads in writes
5. **Generate Report**: Produces actionable insights

### Instrumentation

1. **AST Transformation**: Injects profiling code around expensive operations
2. **Runtime Injection**: Adds ink tracking module with Stylus VM integration
3. **Feature Gated**: Controlled via `ink-profiling` cargo feature
4. **Zero Overhead**: Completely removed in production builds

### Real Profiling

1. **Instrument & Build**: Generates instrumented WASM
2. **Deploy**: Sends WASM to chain as Stylus contract
3. **Execute**: Runs profiling transaction
4. **Measure**: Reads actual ink consumption from Stylus VM
5. **Report**: Displays real measurements vs estimates

## Optimization Categories

Inkwell detects and reports on these categories:

| Category | Example | Typical Ink |
|----------|---------|-------------|
| `storage_read` | `self.balance.get()` | ~1.2M |
| `storage_write` | `self.balance.set(val)` | ~1.5M |
| `storage_write (embedded)` | `self.x.set(self.x.get() + 1)` | ~2.4M |
| `evm_context` | `msg::sender()` | ~200K |
| `event` | `evm::log(Transfer{...})` | ~350K |
| `external_call` | `other.transfer()` | ~2.5M |
| `crypto` | `keccak256(data)` | ~500K |

## Integration

### VS Code Extension

Inkwell generates decoration files for VS Code integration:

```bash
inkwell dip src/contract.rs
# Creates: .inkwell/decorations.json
```

### CI/CD

Fail builds if ink consumption is too high:

```yaml
- name: Analyze Ink Consumption
  run: |
    inkwell dip src/contract.rs --output json > report.json
    TOTAL_INK=$(jq '.functions[0].total_ink' report.json)
    if [ $TOTAL_INK -gt 10000000 ]; then
      echo "Ink consumption too high: $TOTAL_INK"
      exit 1
    fi
```

## Configuration

### Cargo.toml Feature Flag

Add to your contract's `Cargo.toml`:

```toml
[features]
ink-profiling = []
```

### Custom Ink Costs

Modify estimation in `src/analyzer.rs`:

```rust
fn estimate_ink_cost(&self, operation: &str, category: &str) -> u64 {
    match category {
        "storage_read" => 1_200_000,
        "storage_write" => 1_500_000,
        // ... customize costs
        _ => 50_000,
    }
}
```

## Examples

### Analyze ERC20 Token

```bash
inkwell dip examples/erc20/src/contract.rs --function transfer
```

### Profile on Arbitrum Sepolia

```bash
inkwell dip src/contract.rs \
  --profile \
  --rpc-url https://sepolia-rollup.arbitrum.io/rpc \
  --private-key $PRIVATE_KEY \
  --chain-id 421614 \
  --calldata 0x...
```

### Generate Report for Multiple Functions

```bash
for func in transfer approve transferFrom; do
  inkwell dip src/contract.rs --function $func --output json > "report_${func}.json"
done
```

## Testing

Run the test suite:

```bash
cargo test
```

Run specific module tests:

```bash
cargo test --test analyzer
cargo test --test instrumentor
cargo test --test reporter
```

## Contributing

Contributions welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass: `cargo test`
5. Submit a pull request

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Acknowledgments

- Built for [Arbitrum Stylus](https://arbitrum.io/)
- Inspired by gas profilers in Solidity ecosystem
- Theme: "Dipping" into gas analysis like testing litmus with inkwell

---

**Made with ❤️ for the Stylus community**