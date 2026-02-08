# Inkwell - Stylus Ink Profiler for VS Code

Visualize ink costs directly in your editor with inline decorations, gutter icons, and optimization suggestions.

## Features

### Inline Cost Annotations

```rust
pub fn transfer(&mut self, to: Address, amount: U256) -> Result<()> {
    let sender = msg::sender();                             // 💰 200K ink (4%)
    
    let balance = self.balances.get(sender);                // 💰 1.2M ink (27%) 🔥
    
    self.balances.insert(sender, balance - amount);         // 💰 1.5M ink (33%) 🔥
    self.balances.insert(to, self.balances.get(to) + amount); // 💰 1.2M ink (27%) 🔥 💡
}
```

### Gutter Icons

- 🔥 **Hotspot** - Operations consuming > 1M ink
- 💡 **Optimization** - Suggested improvements available

### Status Bar

Shows total ink consumption and hotspot count for quick reference.

### Hover Information

Hover over any line to see detailed ink cost breakdown and optimization suggestions.

## Requirements

- Inkwell CLI must be installed: `cargo install --path ./cli`
- Rust project with Stylus SDK

## Usage

1. Open a Rust file containing a Stylus contract
2. Run command: `Inkwell: Profile Current File` (Cmd/Ctrl+Shift+P)
3. View inline decorations and click 💡 icons for optimization suggestions

## Extension Settings

- `inkwell.autoProfile`: Automatically profile on file save
- `inkwell.decorations.enabled`: Show inline decorations
- `inkwell.decorations.showGutterIcons`: Show gutter icons for hotspots
- `inkwell.cliPath`: Path to inkwell CLI executable

## Commands

- `Inkwell: Profile Current File` - Analyze the current file
- `Inkwell: Clear Decorations` - Remove all visual markers
- `Inkwell: Toggle Auto-Profile on Save` - Enable/disable auto-profiling

## Known Issues

- MVP version uses heuristic analysis rather than TestVM execution
- Currently profiles one function at a time

## Release Notes

### 0.1.0

Initial MVP release:
- Inline cost annotations
- Gutter hotspot icons
- Hover tooltips
- Redundant storage read detection