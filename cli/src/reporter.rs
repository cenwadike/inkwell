// src/reporter.rs
use anyhow::Result;
use colored::Colorize;
use std::collections::HashMap;

use crate::types::*;

/// Reporter for formatting and displaying contract ink analysis results.
///
/// Supports multiple output formats:
/// - `compact`   : concise terminal output with colored highlights (default)
/// - `detailed`  : compact + category breakdown table
/// - `json`      : machine-readable JSON dump
///
/// Also capable of generating VS Code decoration data (inline text, gutter icons,
/// hover tooltips, code actions) for editor integration.
pub struct Reporter {
    /// Output format requested by the user ("compact", "detailed", "json")
    output_format: String,
    /// Whether ANSI color codes should be used in terminal output
    use_color: bool,
}

impl Reporter {
    /// Creates a new reporter with the specified format and color preference.
    ///
    /// # Parameters
    /// - `output_format` - Desired format ("compact", "detailed", "json")
    /// - `_threshold`    - Currently unused (reserved for future filtering)
    /// - `use_color`     - Enable/disable colored terminal output
    pub fn new(output_format: &str, _threshold: u64, use_color: bool) -> Self {
        Self {
            output_format: output_format.to_string(),
            use_color,
        }
    }

    /// Prints the analysis report in the user-selected format.
    ///
    /// Delegates to the appropriate format-specific printer.
    pub fn print_report(&self, analysis: &ContractAnalysis) -> Result<()> {
        match self.output_format.as_str() {
            "json" => self.print_json(analysis),
            "detailed" => self.print_detailed(analysis),
            _ => self.print_compact(analysis),
        }
    }

    /// Outputs the full analysis as pretty-printed JSON to stdout.
    fn print_json(&self, analysis: &ContractAnalysis) -> Result<()> {
        println!("{}", serde_json::to_string_pretty(analysis)?);
        Ok(())
    }

    /// Prints a compact, human-readable terminal report optimized for quick scanning.
    ///
    /// Features:
    /// - Header with contract overview
    /// - Per-function summary (ink usage, gas equivalent)
    /// - Highlighted dry-nib bugs (if any)
    /// - Expensive lines grouped by impact
    /// - Optimization suggestions
    fn print_compact(&self, analysis: &ContractAnalysis) -> Result<()> {
        if self.use_color {
            println!("\n{}", "🧪 INKWELL STAIN REPORT".bright_cyan().bold());
            println!("{}", "━".repeat(60).dimmed());
        } else {
            println!("\nINKWELL STAIN REPORT");
            println!("{}", "=".repeat(60));
        }

        for func in analysis.functions.values() {
            self.print_function_compact(func)?;
        }

        Ok(())
    }

    /// Prints detailed information about detected dry-nib overcharge bugs.
    ///
    /// Dry-nib bugs occur when host calls allocate/charge for more buffer space
    /// than the actual data returned (common with small values in Stylus).
    fn print_dry_nib_bugs(&self, bugs: &[DryNibBug]) -> Result<()> {
        if self.use_color {
            println!("\n{}", "═".repeat(60).bright_magenta());
            println!(
                "  {}",
                "🐛 DRY NIB BUGS DETECTED - HOST CALL OVERHEAD ISSUES"
                    .bright_magenta()
                    .bold()
            );
            println!("{}", "═".repeat(60).bright_magenta());
            println!();
            println!(
                "{}",
                "These operations are charged for more buffer space than data actually returned:"
                    .bright_white()
            );
            println!();
        } else {
            println!("\n{}", "=".repeat(60));
            println!("  DRY NIB BUGS DETECTED - HOST CALL OVERHEAD ISSUES");
            println!("{}", "=".repeat(60));
            println!();
            println!(
                "These operations are charged for more buffer space than data actually returned:"
            );
            println!();
        }

        for (idx, bug) in bugs.iter().enumerate() {
            if self.use_color {
                let severity_icon = match bug.severity.as_str() {
                    "high" => "🚨",
                    "medium" => "⚠️",
                    _ => "ℹ️",
                };

                println!(
                    "  {} Bug #{}: {} at line {}",
                    severity_icon,
                    idx + 1,
                    bug.operation.bright_yellow(),
                    bug.line.to_string().bright_white()
                );
                println!("     │");
                println!(
                    "     │ {} {}",
                    "Operation:".dimmed(),
                    bug.category.bright_cyan()
                );
                println!(
                    "     │ {} {} bytes",
                    "Actual return size:".dimmed(),
                    bug.actual_return_size.to_string().bright_green()
                );
                println!(
                    "     │ {} {} bytes",
                    "Buffer allocated:".dimmed(),
                    bug.buffer_allocated.to_string().bright_red()
                );
                println!(
                    "     │ {} charged for {} bytes of padding!",
                    "Wastage:".bright_red(),
                    (bug.buffer_allocated - bug.actual_return_size)
                        .to_string()
                        .bright_red()
                        .bold()
                );
                println!("     │");
                println!(
                    "     │ {} {:?} ink",
                    "Ink charged (est):".dimmed(),
                    bug.ink_charged_estimate
                );
                println!(
                    "     │ {} {:?} ink",
                    "Fair cost:".dimmed(),
                    bug.expected_fair_cost
                );
                println!(
                    "     │ {} {} ink ({:.1}% overcharge)",
                    "Overcharge:".bright_red().bold(),
                    bug.overcharge_estimate.to_string().bright_red().bold(),
                    (bug.overcharge_estimate as f64 / bug.expected_fair_cost as f64 * 100.0)
                );
                println!("     │");
                println!("     │ {} {}", "Mitigation:".bright_green(), bug.mitigation);
                println!();
            } else {
                println!("  Bug #{}: {} at line {}", idx + 1, bug.operation, bug.line);
                println!("     |");
                println!("     | Operation: {}", bug.category);
                println!(
                    "     | Actual return size: {} bytes",
                    bug.actual_return_size
                );
                println!("     | Buffer allocated: {} bytes", bug.buffer_allocated);
                println!(
                    "     | Wastage: charged for {} bytes of padding!",
                    bug.buffer_allocated - bug.actual_return_size
                );
                println!("     |");
                println!(
                    "     | Ink charged (est): {:?} ink",
                    bug.ink_charged_estimate
                );
                println!("     | Fair cost: {:?} ink", bug.expected_fair_cost);
                println!(
                    "     | Overcharge: {} ink ({:.1}% overcharge)",
                    bug.overcharge_estimate,
                    (bug.overcharge_estimate as f64 / bug.expected_fair_cost as f64 * 100.0)
                );
                println!("     |");
                println!("     | Mitigation: {}", bug.mitigation);
                println!();
            }
        }

        if self.use_color {
            println!("{}", "═".repeat(60).bright_magenta());
        } else {
            println!("{}", "=".repeat(60));
        }

        Ok(())
    }

    /// Prints a more verbose report including per-category ink usage statistics.
    fn print_detailed(&self, analysis: &ContractAnalysis) -> Result<()> {
        self.print_compact(analysis)?;

        for func in analysis.functions.values() {
            if !func.categories.is_empty() {
                if self.use_color {
                    println!("\n{}", "📊 CATEGORY SUMMARY".bright_blue().bold());
                    println!();
                    println!(
                        "{:15} │ {:^10} │ {:^12} │ {:^5} │ {:^10}",
                        "Category", "Operations", "Total Ink", "%", "Avg/Op"
                    );
                    println!("{}", "─".repeat(75).dimmed());
                } else {
                    println!("\nCATEGORY SUMMARY\n");
                    println!(
                        "{:15} | {:^10} | {:^12} | {:^5} | {:^10}",
                        "Category", "Operations", "Total Ink", "%", "Avg/Op"
                    );
                    println!("{}", "-".repeat(75));
                }

                for (category, stats) in &func.categories {
                    let icon = match category.as_str() {
                        "storage_write" | "storage_read" => "🔥",
                        "evm_context" => "🐛",
                        _ => "  ",
                    };

                    if self.use_color {
                        println!(
                            "{:15} │ {:^10} │ {:>12} │ {:>4.0}% │ {:>10}",
                            category,
                            stats.count,
                            format!("{:?}", stats.total_ink),
                            stats.percentage,
                            format!("{:?} {}", stats.avg_per_op, icon)
                        );
                    } else {
                        println!(
                            "{:15} | {:^10} | {:>12} | {:>4.0}% | {:>10}",
                            category,
                            stats.count,
                            format!("{:?}", stats.total_ink),
                            stats.percentage,
                            format!("{:?} {}", stats.avg_per_op, icon)
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Generates decoration data suitable for a VS Code extension.
    ///
    /// Produces:
    /// - Inline decorations (text shown after the line)
    /// - Gutter icons (flame, warning, bug, lightbulb)
    /// - Hover markdown tooltips
    /// - Code actions (quick fixes / refactorings)
    pub fn generate_vscode_decorations(
        &self,
        analysis: &ContractAnalysis,
    ) -> Result<VsCodeDecorations> {
        if analysis.functions.is_empty() {
            return Ok(VsCodeDecorations {
                file: analysis.file.clone(),
                function: "no_functions_detected".to_string(),
                total_ink: 0,
                gas_equivalent: 0,
                decorations: Decorations {
                    inline: vec![],
                    gutter: vec![],
                    hovers: vec![],
                    code_actions: vec![],
                },
            });
        }

        let mut inline_decorations = Vec::new();
        let mut gutter_decorations = Vec::new();
        let mut hover_decorations = Vec::new();
        let mut code_actions = Vec::new();

        let total_ink: u64 = analysis.functions.values().map(|f| f.total_ink).sum();
        let total_gas: u64 = analysis.functions.values().map(|f| f.gas_equivalent).sum();

        for func in analysis.functions.values() {
            let mut line_summary: HashMap<usize, LineSummary> = HashMap::new();

            for op in &func.operations {
                let entry = line_summary.entry(op.line).or_insert(LineSummary {
                    total_ink: 0,
                    op_count: 0,
                    max_severity: "low".to_string(),
                    operations: vec![],
                    representative_name: String::new(),
                });

                entry.total_ink += op.ink;
                entry.op_count += 1;
                entry.operations.push(op);

                if op.severity == "high" && entry.max_severity != "high" {
                    entry.max_severity = "high".to_string();
                } else if op.severity == "medium" && entry.max_severity == "low" {
                    entry.max_severity = "medium".to_string();
                }

                if entry.representative_name.is_empty() && op.entity != "unknown" {
                    entry.representative_name = format!("{}::{}", op.entity, op.operation);
                } else if entry.representative_name.is_empty() {
                    entry.representative_name = op.operation.clone();
                }
            }

            for (line, summary) in line_summary {
                let is_high = summary.max_severity == "high" || summary.total_ink >= 2_000_000;

                let display_ink = if summary.total_ink >= 1_000_000 {
                    format!("{:.1}M", summary.total_ink as f64 / 1_000_000.0)
                } else {
                    format!("{}K", summary.total_ink / 1000)
                };

                let percentage = if func.total_ink > 0 {
                    summary.total_ink as f64 / func.total_ink as f64 * 100.0
                } else {
                    0.0
                };

                let text = if summary.op_count <= 1 {
                    format!("≈ {} ink  {:.0}%", display_ink, percentage)
                } else {
                    format!(
                        "≈ {} ink  {:.0}% [{} ops]",
                        display_ink, percentage, summary.op_count
                    )
                };

                let color = match summary.max_severity.as_str() {
                    "high" => "error",
                    "medium" => "warning",
                    _ => "info",
                };

                inline_decorations.push(InlineDecoration {
                    line,
                    text: if is_high {
                        format!("{} 🔥", text)
                    } else {
                        text
                    },
                    color: color.to_string(),
                });

                if summary.total_ink >= 1_500_000 || summary.max_severity == "high" {
                    gutter_decorations.push(GutterDecoration {
                        line,
                        icon: if summary.max_severity == "high" {
                            "flame".to_string()
                        } else {
                            "warning".to_string()
                        },
                        severity: summary.max_severity.clone(),
                    });
                }

                let mut hover_md = format!(
                    "### Ink Usage on Line {}\n\n**Total:** {} ink  ({:.1}% of function)\n**Severity:** {}\n\n",
                    line,
                    summary.total_ink,
                    percentage,
                    summary.max_severity.to_uppercase()
                );

                if summary.operations.len() == 1 {
                    let op = summary.operations[0];
                    hover_md.push_str(&format!(
                        "**Operation:** `{}`\n**Category:** {}\n**Ink:** {} ({}%)\n",
                        op.operation, op.category, op.ink, op.percentage
                    ));
                } else {
                    hover_md.push_str(&format!("**{} operations:**\n\n", summary.op_count));
                    for (i, op) in summary.operations.iter().enumerate() {
                        hover_md.push_str(&format!(
                            "{}. `{}` — {} ink ({:.1}%)\n   Category: {} | Severity: {}\n",
                            i + 1,
                            op.operation,
                            op.ink,
                            op.percentage,
                            op.category,
                            op.severity
                        ));
                    }
                }

                hover_md.push_str(&format!("\n**Function:** `{}`", func.name));

                hover_decorations.push(HoverDecoration {
                    line,
                    markdown: hover_md,
                });
            }

            // Dry NIB bugs
            for bug in &func.dry_nib_bugs {
                let line = bug.line;

                inline_decorations.push(InlineDecoration {
                    line,
                    text: format!("DRY NIB: ~{}K ink wasted", bug.overcharge_estimate / 1000),
                    color: "error".to_string(),
                });

                gutter_decorations.push(GutterDecoration {
                    line,
                    icon: "bug".to_string(),
                    severity: "error".to_string(),
                });

                let hover_md = format!(
                    "### 🐛 DRY NIB BUG\n\n\
                    **Operation:** `{}`\n\
                    **Actual return:** {} bytes\n\
                    **Buffer allocated:** {} bytes\n\
                    **Wasted:** {} bytes\n\n\
                    **Overcharge:** {} ink ({:.1}%)\n\n\
                    **Fix suggestion:** {}\n\n\
                    **Function:** `{}`",
                    bug.operation,
                    bug.actual_return_size,
                    bug.buffer_allocated,
                    bug.buffer_allocated - bug.actual_return_size,
                    bug.overcharge_estimate,
                    (bug.overcharge_estimate as f64 / bug.expected_fair_cost as f64 * 100.0),
                    bug.mitigation,
                    func.name
                );

                hover_decorations.push(HoverDecoration {
                    line,
                    markdown: hover_md,
                });

                code_actions.push(CodeAction {
                    line,
                    title: format!("Fix dry nib: {}", bug.operation),
                    replacement: Replacement {
                        start_line: line,
                        end_line: line,
                        new_text: format!("// TODO: {}", bug.mitigation),
                    },
                });
            }

            // Optimizations
            for opt in &func.optimizations {
                let line = opt.line;

                gutter_decorations.push(GutterDecoration {
                    line,
                    icon: "lightbulb".to_string(),
                    severity: "warning".to_string(),
                });

                code_actions.push(CodeAction {
                    line,
                    title: opt.title.clone(),
                    replacement: Replacement {
                        start_line: line,
                        end_line: line,
                        new_text: opt.suggested_code.clone(),
                    },
                });
            }
        }

        Ok(VsCodeDecorations {
            file: analysis.file.clone(),
            function: "All Functions".to_string(),
            total_ink,
            gas_equivalent: total_gas,
            decorations: Decorations {
                inline: inline_decorations,
                gutter: gutter_decorations,
                hovers: hover_decorations,
                code_actions,
            },
        })
    }

    /// Prints compact summary for a single function.
    ///
    /// Includes:
    /// - Function signature and total ink/gas
    /// - Dry-nib bugs (if present)
    /// - Most expensive lines (threshold: ≥800K ink)
    /// - Optimization suggestions
    fn print_function_compact(&self, func: &FunctionAnalysis) -> Result<()> {
        let use_color = self.use_color;

        if use_color {
            println!("\n🎯 {}", func.signature.bright_white().bold());
            println!(
                "💰 Total: {} ink  (≈ {} gas)",
                func.total_ink.to_string().bright_yellow(),
                func.gas_equivalent.to_string().bright_yellow()
            );
            println!("{}", "─".repeat(60).dimmed());
        } else {
            println!("\nFunction: {}", func.signature);
            println!(
                "Total: {} ink (≈ {} gas)",
                func.total_ink, func.gas_equivalent
            );
            println!("{}", "=".repeat(60));
        }

        if !func.dry_nib_bugs.is_empty() {
            self.print_dry_nib_bugs(&func.dry_nib_bugs)?;
        }

        let mut line_summary: HashMap<usize, LineSummary> = HashMap::new();

        for op in &func.operations {
            let entry = line_summary.entry(op.line).or_insert(LineSummary {
                total_ink: 0,
                op_count: 0,
                max_severity: "low".to_string(),
                operations: vec![],
                representative_name: String::new(),
            });

            entry.total_ink += op.ink;
            entry.op_count += 1;
            entry.operations.push(op);

            if op.severity == "high" {
                entry.max_severity = "high".to_string();
            } else if op.severity == "medium" && entry.max_severity == "low" {
                entry.max_severity = "medium".to_string();
            }

            if entry.representative_name.is_empty() {
                entry.representative_name = if op.entity != "unknown" {
                    format!("{}::{}", op.entity, op.operation)
                } else {
                    op.operation.clone()
                };
            }
        }

        let mut sorted_lines: Vec<(usize, &LineSummary)> = line_summary
            .iter()
            .map(|(line, summary)| (*line, summary))
            .collect();
        sorted_lines.sort_by(|a, b| b.1.total_ink.cmp(&a.1.total_ink));

        if use_color {
            println!("\n{}", "🔥 Expensive Lines".bright_red().bold());
        } else {
            println!("\nExpensive Lines");
        }

        for (line, summary) in sorted_lines {
            if summary.total_ink < 800_000 {
                continue;
            }

            let percentage = if func.total_ink > 0 {
                summary.total_ink as f64 / func.total_ink as f64 * 100.0
            } else {
                0.0
            };

            let ink_display = if summary.total_ink >= 1_000_000 {
                format!("{:.1}M", summary.total_ink as f64 / 1_000_000.0)
            } else {
                format!("{}K", summary.total_ink / 1000)
            };

            let bar_width = (percentage / 5.0).min(20.0) as usize;
            let bar = "█".repeat(bar_width);

            let severity_marker = if summary.max_severity == "high" {
                " 🔥".bright_red().to_string()
            } else if summary.max_severity == "medium" {
                " ⚠️".yellow().to_string()
            } else {
                "".to_string()
            };

            if use_color {
                println!(
                    "  Line {:4} │ {:<38} {:>6} ink  {:>5.1}%  {}{}",
                    line.to_string().bright_white(),
                    summary.representative_name.bright_white(),
                    ink_display.bright_yellow(),
                    percentage,
                    bar.bright_red(),
                    severity_marker
                );
            } else {
                println!(
                    "  Line {:4} | {:<38} {:>6} ink  {:>5.1}%  {}",
                    line, summary.representative_name, ink_display, percentage, bar
                );
            }
        }

        if !func.optimizations.is_empty() {
            if use_color {
                println!("\n{}", "💡 Optimizations".bright_green().bold());
            } else {
                println!("\nOptimizations");
            }

            for opt in &func.optimizations {
                if use_color {
                    println!(
                        "  Line {:4} │ {}  (savings ~{}K ink)",
                        opt.line.to_string().bright_white(),
                        opt.title.bright_yellow(),
                        (opt.estimated_savings_ink / 1000).to_string().bright_cyan()
                    );
                } else {
                    println!(
                        "  Line {:4} | {}  (savings ~{}K ink)",
                        opt.line,
                        opt.title,
                        opt.estimated_savings_ink / 1000
                    );
                }
            }
        }

        if use_color {
            println!("{}", "─".repeat(60).dimmed());
        } else {
            println!("{}", "-".repeat(60));
        }

        Ok(())
    }
}
