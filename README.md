# Inkwell
Inkwell is a profiler for Arbitrum Stylus smart contracts. Analyze gas consumption, identify optimization opportunities, and visualize costs directly in your editor.

## Features

- 🔍 **Accurate Ink Profiling** - Analyze ink consumption for Stylus contracts
- 🔥 **Hotspot Detection** - Identify expensive operations automatically  
- 💡 **Optimization Suggestions** - Get actionable recommendations to reduce costs
- 📊 **Multiple Output Formats** - Compact, detailed, and JSON reports
- 🎨 **VS Code Integration** - Inline decorations, gutter icons, and hover information

## Quick Start

### 1. Install CLI

```bash
cd cli
cargo install --path .
```

### 2. Profile a Contract

```bash
inkwell dip src/token.rs --function transfer
```

Example output:
```
🧪 INKWELL STAIN REPORT
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🎯 Function: transfer(...)
💰 Total Ink: 4,500,000 (≈ 450 gas)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🔥 HOTSPOTS (Operations > 1M ink)

  Line  24  │  storage_write      1.5M  ████████████████▌  33%
  Line  18  │  storage_read       1.2M  ████████████▌      27%
  Line  26  │  storage_write      1.2M  ████████████▌      27%

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

💡 OPTIMIZATION OPPORTUNITIES

  Line 26 │ Redundant Storage Read
          │ Storage read detected within insert operation...
          │
          │ Suggestion:
          │   // Cache the value first:
          │   // let cached_value = self.storage.get(key);
          │   // self.storage.insert(key, cached_value + amount);
          │
          │ 💰 Potential savings: ~600K ink (13% reduction)
```

### 3. Install VS Code Extension

```bash
cd vscode
npm install
npm run compile
```

Then press F5 in VS Code to launch the extension in development mode.

## CLI Usage

```bash
inkwell dip <FILE> [OPTIONS]

OPTIONS:
  -f, --function <NAME>    Profile specific function
  -o, --output <TYPE>      Output format: compact|detailed|json
      --threshold <INK>    Only show operations above threshold
      --no-color           Disable colored output
```

## VS Code Extension

### Commands

- `Inkwell: Profile Current File` - Profile the active Rust file
- `Inkwell: Clear Decorations` - Remove all inline decorations
- `Inkwell: Toggle Auto-Profile on Save` - Enable/disable automatic profiling

### Settings

```json
{
  "inkwell.autoProfile": false,
  "inkwell.decorations.enabled": true,
  "inkwell.decorations.showGutterIcons": true,
  "inkwell.cliPath": "inkwell"
}
```

## How It Works

Inkwell analyzes your Stylus contracts using heuristics to estimate ink consumption:

1. **Parse** - Uses `syn` to parse Rust AST
2. **Analyze** - Identifies storage operations, EVM calls, and events
3. **Estimate** - Assigns ink costs based on operation types:
   - Storage write: ~1.5M ink
   - Storage read: ~1.2M ink
   - Event emission: ~350K ink
   - EVM calls: ~200K ink
4. **Report** - Generates insights and optimization suggestions

## Optimization Patterns

Inkwell detects common optimization opportunities:

### Redundant Storage Reads

❌ **Before** (1.2M extra ink):
```rust
self.balances.insert(to, self.balances.get(to) + amount);
```

✅ **After**:
```rust
let to_balance = self.balances.get(to);
self.balances.insert(to, to_balance + amount);
```

## Development

### Project Structure

```
inkwell/
├── cli/                  # Rust CLI tool
│   └── src/
│       ├── main.rs       # Entry point
│       ├── analyzer.rs   # AST analysis
│       ├── reporter.rs   # Output generation
│       └── types.rs      # Type definitions
├── vscode-extension/     # VS Code extension
│   └── src/
│       ├── extension.ts  # Extension entry
│       ├── profiler.ts   # CLI integration
│       └── decorations.ts # Visual decorations
└── test-contracts/       # Sample contracts
```

### Running Tests

```bash
# CLI tests
cd cli
cargo test

# Extension tests  
cd vscode-extension
npm test
```

## Roadmap

- [ ] Support for more optimization patterns
- [ ] Batch profiling for entire projects
- [ ] Historical tracking and regression detection

## Contributing

Contributions welcome! Please open an issue or PR.

## License

Apache 2.0
