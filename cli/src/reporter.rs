// src/reporter.rs
use anyhow::Result;
use colored::Colorize;

use crate::types::*;

pub struct Reporter {
    output_format: String,
    use_color: bool,
}

impl Reporter {
    pub fn new(output_format: &str, _threshold: u64, use_color: bool) -> Self {
        Self {
            output_format: output_format.to_string(),
            use_color,
        }
    }

    pub fn print_report(&self, analysis: &ContractAnalysis) -> Result<()> {
        match self.output_format.as_str() {
            "json" => self.print_json(analysis),
            "detailed" => self.print_detailed(analysis),
            _ => self.print_compact(analysis),
        }
    }

    fn print_json(&self, analysis: &ContractAnalysis) -> Result<()> {
        println!("{}", serde_json::to_string_pretty(analysis)?);
        Ok(())
    }

    fn print_compact(&self, analysis: &ContractAnalysis) -> Result<()> {
        if self.use_color {
            println!("\n{}", "🧪 INKWELL STAIN REPORT".bright_cyan().bold());
            println!("{}", "━".repeat(60).dimmed());
        } else {
            println!("\nINKWELL STAIN REPORT");
            println!("{}", "=".repeat(60));
        }

        for func in &analysis.functions {
            self.print_function_compact(func)?;
        }

        Ok(())
    }

    fn print_function_compact(&self, func: &FunctionAnalysis) -> Result<()> {
        if self.use_color {
            println!("\n🎯 Function: {}", func.signature.bright_white());
            println!(
                "💰 Total Ink: {} (≈ {} gas)",
                format!("{:?}", func.total_ink).bright_yellow(),
                func.gas_equivalent.to_string().bright_yellow()
            );
            println!("\n{}", "━".repeat(60).dimmed());
        } else {
            println!("\nFunction: {}", func.signature);
            println!(
                "Total Ink: {:?} (≈ {} gas)",
                func.total_ink, func.gas_equivalent
            );
            println!("\n{}", "=".repeat(60));
        }

        // Print dry nib bugs FIRST (highest priority)
        if !func.dry_nib_bugs.is_empty() {
            self.print_dry_nib_bugs(&func.dry_nib_bugs)?;
        }

        // Print hotspots
        if !func.hotspots.is_empty() {
            if self.use_color {
                println!(
                    "\n{}",
                    "🔥 HOTSPOTS (Operations > 1M ink)".bright_red().bold()
                );
                println!();
            } else {
                println!("\nHOTSPOTS (Operations > 1M ink)\n");
            }

            for hotspot in &func.hotspots {
                if let Some(op) = func.operations.iter().find(|o| o.line == hotspot.line) {
                    let bar_width = (op.percentage / 2.0) as usize;
                    let bar = "█".repeat(bar_width);

                    if self.use_color {
                        println!(
                            "  Line {:3}  │  {:30}  {}  {}  {:>3}%",
                            op.line.to_string().bright_white(),
                            truncate(&op.operation, 30),
                            format!("{:.1}M", op.ink as f64 / 1_000_000.0).bright_yellow(),
                            bar.bright_red(),
                            format!("{:.0}", op.percentage).bright_white()
                        );
                    } else {
                        println!(
                            "  Line {:3}  |  {:30}  {:.1}M  {}  {:>3}%",
                            op.line,
                            truncate(&op.operation, 30),
                            op.ink as f64 / 1_000_000.0,
                            bar,
                            format!("{:.0}", op.percentage)
                        );
                    }
                }
            }
        }

        // Print optimizations
        if !func.optimizations.is_empty() {
            if self.use_color {
                println!("\n{}", "━".repeat(60).dimmed());
                println!(
                    "\n{}",
                    "💡 OPTIMIZATION OPPORTUNITIES".bright_green().bold()
                );
                println!();
            } else {
                println!("\n{}", "=".repeat(60));
                println!("\nOPTIMIZATION OPPORTUNITIES\n");
            }

            for opt in &func.optimizations {
                if self.use_color {
                    println!(
                        "  {} │ {}",
                        format!("Line {}", opt.line).bright_white(),
                        opt.title.bright_yellow()
                    );
                    println!("  {} │ {}", " ".repeat(8), opt.description.dimmed());
                    println!("  {} │", " ".repeat(8));
                    println!("  {} │ Suggestion:", " ".repeat(8));
                    for line in opt.suggested_code.lines() {
                        println!("  {} │   {}", " ".repeat(8), line.green());
                    }
                    println!("  {} │", " ".repeat(8));
                    println!(
                        "  {} │ 💰 Potential savings: ~{}K ink ({}% reduction)",
                        " ".repeat(8),
                        opt.estimated_savings_ink / 1000,
                        format!("{:.0}", opt.estimated_savings_percentage).bright_cyan()
                    );
                    println!();
                } else {
                    println!("  Line {} | {}", opt.line, opt.title);
                    println!("  {}   | {}", " ".repeat(4), opt.description);
                    println!("  {}   |", " ".repeat(4));
                    println!("  {}   | Suggestion:", " ".repeat(4));
                    for line in opt.suggested_code.lines() {
                        println!("  {}   |   {}", " ".repeat(4), line);
                    }
                    println!("  {}   |", " ".repeat(4));
                    println!(
                        "  {}   | Potential savings: ~{}K ink ({}% reduction)",
                        " ".repeat(4),
                        opt.estimated_savings_ink / 1000,
                        format!("{:.0}", opt.estimated_savings_percentage)
                    );
                    println!();
                }
            }
        }

        if self.use_color {
            println!("{}", "━".repeat(60).dimmed());
        } else {
            println!("{}", "=".repeat(60));
        }

        Ok(())
    }

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

    fn print_detailed(&self, analysis: &ContractAnalysis) -> Result<()> {
        // Similar to compact but with category breakdown
        self.print_compact(analysis)?;

        for func in &analysis.functions {
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
                        "evm_context" => "🐛", // Dry nib prone!
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

    pub fn generate_vscode_decorations(
        &self,
        analysis: &ContractAnalysis,
    ) -> Result<VsCodeDecorations> {
        let func = &analysis.functions[0];

        let mut inline_decorations = Vec::new();
        let mut gutter_decorations = Vec::new();
        let mut hover_decorations = Vec::new();
        let mut code_actions = Vec::new();

        // Generate decorations for operations
        for op in &func.operations {
            let color = match op.severity.as_str() {
                "high" => "high",
                "medium" => "medium",
                _ => "low",
            };

            let text = format!(
                " 💰 {:.1}{}K ink ({}%){}",
                if op.ink > 1_000_000 {
                    op.ink as f64 / 1_000_000.0
                } else {
                    op.ink as f64 / 1_000.0
                },
                if op.ink > 1_000_000 { "M" } else { "" },
                format!("{:.0}", op.percentage),
                if op.severity == "high" { " 🔥" } else { "" }
            );

            inline_decorations.push(InlineDecoration {
                line: op.line,
                text,
                color: color.to_string(),
            });

            if op.ink > 1_000_000 {
                gutter_decorations.push(GutterDecoration {
                    line: op.line,
                    icon: "flame".to_string(),
                    severity: "high".to_string(),
                });
            }

            let hover_md = format!(
                "### 💰 Ink Cost: {:?} ({:.0}%)\n\n**Operation:** `{}`  \n**Category:** {}  \n**Severity:** {}",
                op.ink, op.percentage, op.operation, op.category, op.severity
            );

            hover_decorations.push(HoverDecoration {
                line: op.line,
                markdown: hover_md,
            });
        }

        // Generate decorations for dry nib bugs
        for bug in &func.dry_nib_bugs {
            // Add special decoration
            let bug_inline = InlineDecoration {
                line: bug.line,
                text: format!(
                    " 🐛 DRY NIB: {} ink wasted ({}→{} bytes)",
                    bug.overcharge_estimate, bug.actual_return_size, bug.buffer_allocated
                ),
                color: "error".to_string(),
            };
            inline_decorations.push(bug_inline);

            // Add bug icon in gutter
            gutter_decorations.push(GutterDecoration {
                line: bug.line,
                icon: "bug".to_string(),
                severity: "error".to_string(),
            });

            // Add detailed hover
            let hover_md = format!(
                "### 🐛 DRY NIB BUG DETECTED\n\n\
                **Operation:** `{}`  \n\
                **Actual return:** {} bytes  \n\
                **Buffer allocated:** {} bytes  \n\
                **Wastage:** {} bytes  \n\n\
                **Ink overcharge:** {} ({:.1}%)  \n\n\
                💡 **Mitigation:** {}",
                bug.operation,
                bug.actual_return_size,
                bug.buffer_allocated,
                bug.buffer_allocated - bug.actual_return_size,
                bug.overcharge_estimate,
                (bug.overcharge_estimate as f64 / bug.expected_fair_cost as f64 * 100.0),
                bug.mitigation
            );

            hover_decorations.push(HoverDecoration {
                line: bug.line,
                markdown: hover_md,
            });

            // Add code action
            code_actions.push(CodeAction {
                line: bug.line,
                title: format!("🐛 Fix dry nib bug: {}", bug.operation),
                replacement: Replacement {
                    start_line: bug.line,
                    end_line: bug.line,
                    new_text: format!("// {}", bug.mitigation),
                },
            });
        }

        // Generate code actions for optimizations
        for opt in &func.optimizations {
            code_actions.push(CodeAction {
                line: opt.line,
                title: format!("💡 {}", opt.title),
                replacement: Replacement {
                    start_line: opt.line,
                    end_line: opt.line,
                    new_text: opt.suggested_code.clone(),
                },
            });

            gutter_decorations.push(GutterDecoration {
                line: opt.line,
                icon: "lightbulb".to_string(),
                severity: "warning".to_string(),
            });
        }

        Ok(VsCodeDecorations {
            file: analysis.file.clone(),
            function: func.name.clone(),
            total_ink: func.total_ink,
            gas_equivalent: func.gas_equivalent,
            decorations: Decorations {
                inline: inline_decorations,
                gutter: gutter_decorations,
                hovers: hover_decorations,
                code_actions,
            },
        })
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
