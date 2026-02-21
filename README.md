# Inkwell 🧪

> **Forensic ink profiling for Arbitrum Stylus contracts.**  
> Map WASM ink consumption back to Rust source lines. Catch host-call overcharges. Ship cheaper contracts.

[![License: Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-blue.svg)](LICENSE)
[![Built for Arbitrum Stylus](https://img.shields.io/badge/Built%20for-Arbitrum%20Stylus-1b6fc8)](https://arbitrum.io/stylus)
[![Rust](https://img.shields.io/badge/Rust-2024%20Edition-f74c00)](https://www.rust-lang.org/)

---

## The Problem

Stylus (WASM) contracts execute in an ink-metered environment. Every storage read, every host-call, every nested mapping access costs ink — but the Stylus VM charges a **buffer overhead** that has nothing to do with the actual data returned. A 20-byte `msg::sender()` call gets charged for a 64-byte buffer. A nested `mapping.get().get()` double-charges the load overhead.

These aren't bugs in your contract logic. They're **"Dry Nib" bugs** — silent overcharges baked into how Stylus allocates return buffers for host-calls.

Before Inkwell, you had no way to see them.

---

## What Inkwell Does

Inkwell is a **Forensic Profiler for Stylus contracts**. It parses your Rust source via `syn`, walks the AST, and produces a machine-readable report mapping ink consumption to exact source lines — including:

- **Ink cost per operation** with percentage breakdowns
- **Gas equivalents** translated from raw ink values
- **Dry Nib bug detection** — identifies host-calls where buffer allocation exceeds actual return size
- **Caching optimization suggestions** — catches repeated storage reads of the same field
- **Hotspot ranking** — sorts operations by ink impact across the function
- **CI/CD-ready JSON output** — `ink-report.json` with a standardized schema
- **Runtime instrumentation mode** — inject ink probes for on-chain profiling

---

## Repository Structure

```
inkwell/
├── cli/                     # The Inkwell CLI tool (stylus-inkwell binary)
│   ├── src/
│   │   ├── analyzer.rs      # AST-based static ink analyzer
│   │   ├── instrumentor.rs  # Runtime probe injection engine
│   │   ├── reporter.rs      # Terminal + JSON + VS Code decoration output
│   │   ├── types.rs         # Shared data types and serialization
│   │   └── main.rs          # CLI entry point (clap subcommands)
│   └── README.md            # CLI usage guide
│
├── swap/                    # Example Stylus contract (token swap market)
│   └── src/lib.rs           # Profiling target — run Inkwell against this
│
├── ink-report.json          # Sample output from analyzing the swap contract
└── README.md                # This file
```

---

## Quick Start

### Install

#### From Source

```bash
git clone https://github.com/cenwadike/inkwell
cd inkwell/cli
cargo build --release
```

The binary will be at `cli/target/release/stylus-inkwell`.

#### From Cargo

```bash
cargo install stylus-inkwell 
```

### Analyze a Contract

```bash
# Static analysis — all public functions
stylus-inkwell dip ../swap/src/lib.rs

# Analyze a specific function
stylus-inkwell dip ../swap/src/lib.rs --function create_market

# Full JSON output (CI/CD ready)
stylus-inkwell dip ../swap/src/lib.rs --output json

# Detailed category breakdown
stylus-inkwell dip ../swap/src/lib.rs --output detailed
```

### Instrument for Runtime Profiling

```bash
# Inject ink probes into source
stylus-inkwell instrument ../swap/src/lib.rs --output instrumented.rs

# Profile on-chain (requires running Stylus node)
stylus-inkwell dip ../swap/src/lib.rs \
  --profile \
  --rpc-url http://localhost:8547 \
  --private-key 0xYOUR_KEY \
  --calldata 0xCALLDATA
```

---

## The Dry Nib Bug

The Stylus VM charges storage reads based on the **buffer it allocates**, not the data it returns. For small return values (20-byte addresses, single `bool`s, `uint64` fields), this creates a systematic overcharge.

```
Storage read: self.initialized.get()
  Actual return:   1 byte
  Buffer charged: 32 bytes
  Overcharge:    ~1.2M ink  (100% excess)
```

Inkwell detects these automatically by:

1. Counting `.get()` depth on nested mappings
2. Flagging operations where `ink_charged >> expected_fair_cost`
3. Suggesting caching patterns to eliminate redundant reads

---

## Output: `ink-report.json`

Every `dip` run writes a standardized JSON report to the project root:

```json
{
  "contract_name": "Contract",
  "file": "swap/src/lib.rs",
  "functions": {
    "create_market": {
      "total_ink": 25800000,
      "gas_equivalent": 2580,
      "dry_nib_bugs": [...],
      "optimizations": [...],
      "hotspots": [...],
      "operations": [...]
    }
  }
}
```

Drop this into [JSON Hero](https://jsonhero.io) for a visual audit. Feed it into CI/CD to gate deployments on ink budget. Pipe it to your auditor.

---

## Example: Analyzing the Swap Contract

The `swap/` directory contains a production-style fixed-rate token swap contract. Running Inkwell against it reveals:

```
🧪 INKWELL STAIN REPORT
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🎯 pub fn create_market(&mut self, ...)
💰 Total: 25,800,000 ink  (≈ 2,580 gas)

🐛 DRY NIB BUGS DETECTED
  Bug #1: indexes (nested mapping) at line 168
     | Charged:    4,800,000 ink
     | Fair cost:  1,200,000 ink
     | Overcharge: 3,600,000 ink  (300%)
     | Fix: Cache outer mapping result before inner .get()

🔥 Expensive Lines
  Line  168 │ indexes::nested_map_get   4.8M ink  18.6%  ████████████████████
  Line  186 │ markets::map::insert      4.5M ink  17.4%  █████████████████
  Line  195 │ indexes::map::upsert      3.9M ink  15.1%  ██████████████

💡 Optimizations
  Line  168 │ Cache repeated storage read: self.indexes  (savings ~39,600K ink)
```

---

## Use Cases

**For Developers**  
Understand exactly where your contract burns ink before deployment. Inkwell shows you what to cache, what to restructure, and what the Stylus VM is silently charging you for.

**For Auditors**  
The JSON report is a structured, line-level audit artifact. Severity levels, overcharge estimates, and mitigation suggestions are machine-readable and CI-compatible.

**For Protocol Teams**  
Integrate `stylus-inkwell dip --output json` into your build pipeline. Gate PRs on ink budget regressions. Track gas costs across contract versions.

---

## How It Works

```
Source (.rs)
    │
    ▼
syn::parse_file()          ← Parse Rust AST
    │
    ▼
ContractVisitor (Visit)    ← Walk impl blocks, detect public/external fns
    │
    ▼
analyze_function()         ← Per-statement operation detection
    │
    ├── detect_storage_read()    → map::get, nested_map_get
    ├── detect_storage_write()   → map::insert, map::upsert
    ├── detect_dry_nib_bugs()    → buffer overcharge detection
    ├── detect_optimizations()   → repeated-read caching suggestions
    └── calculate_categories()   → per-category ink aggregation
    │
    ▼
ContractAnalysis (JSON)    ← Serialized report
    │
    ├── ink-report.json           (project root)
    └── .inkwell/decorations.json (VS Code integration data)
```

The ink cost model is based on documented and observed Stylus VM behavior. Static analysis estimates; runtime instrumentation (`--profile` mode) measures.

---

## Roadmap

- [ ] Multi-file contract support (workspace-level analysis)
- [ ] VS Code extension consuming `.inkwell/decorations.json`
- [ ] Differential reports across git commits

---

## Contributing

Issues and PRs welcome. If you're building on Stylus and hit a contract pattern Inkwell doesn't detect, open an issue with the snippet.

---

## License

Apache-2.0 — see [LICENSE](LICENSE).

---

*Built by [Kombi](https://github.com/cenwadike) for the Arbitrum Stylus ecosystem.*
