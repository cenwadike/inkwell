use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level result of a contract analysis.
///
/// Contains the detected contract name, source file path, and a map of
/// all analyzed functions with their ink/gas metrics, operations, optimizations,
/// hotspots, and detected issues (especially dry-nib bugs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractAnalysis {
    /// Name of the contract (usually extracted from `pub struct …`)
    pub contract_name: String,
    /// Path to the analyzed source file (relative or absolute)
    pub file: String,
    /// Map of function name → detailed analysis
    pub functions: HashMap<String, FunctionAnalysis>,
}

/// Detailed analysis of a single function (typically a public/external entry point).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionAnalysis {
    /// Function name (without parameters)
    pub name: String,
    /// Full signature (as stringified Rust syntax)
    pub signature: String,
    /// Approximate starting line number in source file (1-based)
    pub start_line: usize,
    /// Estimated total ink consumption (including penalties for storage ops)
    pub total_ink: u64,
    /// Rough gas equivalent (total_ink / 10_000)
    pub gas_equivalent: u64,
    /// All detected expensive operations with per-op metrics
    pub operations: Vec<Operation>,
    /// Aggregated statistics grouped by operation category
    pub categories: HashMap<String, CategoryStats>,
    /// Suggested optimizations (mainly caching repeated reads)
    pub optimizations: Vec<Optimization>,
    /// Most ink-expensive individual operations (sorted descending)
    pub hotspots: Vec<Hotspot>,
    /// Detected "dry nib" overcharge bugs (buffer waste on host calls)
    pub dry_nib_bugs: Vec<DryNibBug>,
}

/// Single detected expensive operation (storage read/write, host call, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    /// Source line where the operation occurs (1-based)
    pub line: usize,
    /// Column (currently always 0 — span info not fully used)
    pub column: usize,
    /// Original source snippet (quoted via syn/quote)
    pub code: String,
    /// Classified operation name (e.g. "map::get", "nested_map_get", "msg_sender")
    pub operation: String,
    /// Storage entity/field name if applicable ("balances", "allowances", etc.)
    pub entity: String,
    /// Estimated ink cost for this operation
    pub ink: u64,
    /// Percentage of total function ink this operation represents
    pub percentage: f64,
    /// Broad category (storage_read, storage_write, evm_context, event, etc.)
    pub category: String,
    /// Severity level ("high", "medium", "low")
    pub severity: String,
}

/// Temporary struct used during reporting to aggregate operations by line.
#[derive(Debug)]
pub struct LineSummary<'a> {
    /// Sum of ink costs for all operations on this line
    pub total_ink: u64,
    /// Number of individual operations on this line
    pub op_count: usize,
    /// Highest severity among operations on this line
    pub max_severity: String,
    /// References to the original Operation structs
    pub operations: Vec<&'a Operation>,
    /// Clean representative name for display (e.g. "balances::get")
    pub representative_name: String,
}

/// Statistics for one category of operations within a function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryStats {
    /// Number of operations in this category
    pub count: usize,
    /// Sum of ink costs for this category
    pub total_ink: u64,
    /// Percentage of the function's total ink used by this category
    pub percentage: f64,
    /// Average ink per operation in this category
    pub avg_per_op: u64,
}

/// Suggested optimization opportunity (mainly repeated storage reads).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Optimization {
    /// Unique identifier (e.g. "cache_balances")
    pub id: String,
    /// Line number where the optimization should be applied
    pub line: usize,
    /// Severity level ("medium", "high")
    pub severity: String,
    /// Short human-readable title
    pub title: String,
    /// Detailed explanation of the issue and savings
    pub description: String,
    /// Current problematic code pattern (for display)
    pub current_code: String,
    /// Suggested replacement code snippet
    pub suggested_code: String,
    /// Estimated ink savings if applied
    pub estimated_savings_ink: u64,
    /// Estimated percentage reduction in function ink
    pub estimated_savings_percentage: f64,
    /// Confidence in the suggestion ("high", "medium")
    pub confidence: String,
}

/// High-ink individual operation (used to highlight hotspots).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hotspot {
    /// Source line number
    pub line: usize,
    /// Ink cost of this single operation
    pub ink: u64,
    /// Operation name/description
    pub operation: String,
    /// Rank among all hotspots in the function (1 = most expensive)
    pub rank: usize,
}

/// Represents a "dry nib" bug: Stylus host calls often charge for a full buffer
/// (e.g. 64 bytes) even when far less data is returned (e.g. 20-byte address).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DryNibBug {
    /// Line number where the problematic call occurs
    pub line: usize,
    /// Operation that triggered the detection (e.g. "storage_read", "msg_sender")
    pub operation: String,
    /// Category of the operation
    pub category: String,
    /// Estimated ink charged by Stylus (including buffer overhead)
    pub ink_charged_estimate: u64,
    /// Actual size of data returned (in bytes)
    pub actual_return_size: usize,
    /// Size of buffer allocated/charged by Stylus
    pub buffer_allocated: usize,
    /// Fair/expected cost based on actual return size
    pub expected_fair_cost: u64,
    /// Estimated overcharge amount
    pub overcharge_estimate: u64,
    /// Severity level ("high", "medium")
    pub severity: String,
    /// Suggested fix or mitigation strategy
    pub mitigation: String,
}

// ────────────────────────────────────────────────────────────────────────────────
// VS Code / Editor Integration Types
// ────────────────────────────────────────────────────────────────────────────────

/// Root structure for VS Code decoration data (sent to extension via JSON).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsCodeDecorations {
    /// Analyzed source file path
    pub file: String,
    /// Function name (or "All Functions")
    pub function: String,
    /// Sum of ink across all functions
    pub total_ink: u64,
    /// Sum of gas equivalents
    pub gas_equivalent: u64,
    /// All decorations to apply in the editor
    pub decorations: Decorations,
}

/// Collection of different decoration types for one file/function.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Decorations {
    /// Text shown inline after the line
    pub inline: Vec<InlineDecoration>,
    /// Icons/symbols shown in the gutter
    pub gutter: Vec<GutterDecoration>,
    /// Hover tooltips (markdown supported)
    pub hovers: Vec<HoverDecoration>,
    /// Quick-fix / refactor suggestions
    pub code_actions: Vec<CodeAction>,
}

/// Inline text decoration (shown to the right of the line).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineDecoration {
    /// 1-based line number
    pub line: usize,
    /// Text to display (e.g. "1.2M ink  42%")
    pub text: String,
    /// Color theme key ("error", "warning", "info")
    pub color: String,
}

/// Gutter (left margin) icon decoration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GutterDecoration {
    /// 1-based line number
    pub line: usize,
    /// Icon name (e.g. "flame", "bug", "lightbulb", "warning")
    pub icon: String,
    /// Severity level ("high"/"error", "medium"/"warning")
    pub severity: String,
}

/// Hover tooltip content (shown when mouse is over the line).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoverDecoration {
    /// 1-based line number
    pub line: usize,
    /// Markdown-formatted content
    pub markdown: String,
}

/// Quick-fix / code action suggestion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeAction {
    /// 1-based line number to apply the action
    pub line: usize,
    /// Title shown in the lightbulb menu
    pub title: String,
    /// Replacement to apply
    pub replacement: Replacement,
}

/// Text replacement instruction for a code action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replacement {
    /// Starting line (inclusive)
    pub start_line: usize,
    /// Ending line (inclusive)
    pub end_line: usize,
    /// New text to insert in place of the range
    pub new_text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_analysis_creation() {
        let analysis = ContractAnalysis {
            contract_name: "TestContract".to_string(),
            file: "src/lib.rs".to_string(),
            functions: HashMap::new(),
        };

        assert_eq!(analysis.contract_name, "TestContract");
        assert_eq!(analysis.file, "src/lib.rs");
        assert_eq!(analysis.functions.len(), 0);
    }

    #[test]
    fn test_operation_serialization() {
        let op = Operation {
            line: 42,
            column: 10,
            code: "self.balance.get()".to_string(),
            operation: "storage_read".to_string(),
            entity: "balance".to_string(),
            ink: 1_200_000,
            percentage: 25.5,
            category: "storage".to_string(),
            severity: "high".to_string(),
        };

        let json = serde_json::to_string(&op).unwrap();
        let deserialized: Operation = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.line, 42);
        assert_eq!(deserialized.ink, 1_200_000);
        assert_eq!(deserialized.operation, "storage_read");
    }

    #[test]
    fn test_category_stats_calculation() {
        let stats = CategoryStats {
            count: 5,
            total_ink: 5_000_000,
            percentage: 50.0,
            avg_per_op: 1_000_000,
        };

        assert_eq!(stats.count, 5);
        assert_eq!(stats.total_ink, 5_000_000);
        assert_eq!(stats.avg_per_op, 1_000_000);
        assert_eq!(stats.avg_per_op * stats.count as u64, stats.total_ink);
    }

    #[test]
    fn test_hotspot_ordering() {
        let mut hotspots = vec![
            Hotspot {
                line: 10,
                ink: 1_500_000,
                operation: "storage_write".to_string(),
                rank: 0,
            },
            Hotspot {
                line: 20,
                ink: 2_500_000,
                operation: "external_call".to_string(),
                rank: 0,
            },
            Hotspot {
                line: 15,
                ink: 2_000_000,
                operation: "storage_read".to_string(),
                rank: 0,
            },
        ];

        hotspots.sort_by(|a, b| b.ink.cmp(&a.ink));
        for (idx, hotspot) in hotspots.iter_mut().enumerate() {
            hotspot.rank = idx + 1;
        }

        assert_eq!(hotspots[0].rank, 1);
        assert_eq!(hotspots[0].ink, 2_500_000);
        assert_eq!(hotspots[1].rank, 2);
        assert_eq!(hotspots[2].rank, 3);
    }

    #[test]
    fn test_optimization_structure() {
        let opt = Optimization {
            id: "cache_balance".to_string(),
            line: 42,
            severity: "high".to_string(),
            title: "Cache storage read".to_string(),
            description: "Balance is read multiple times".to_string(),
            current_code: "let x = self.balance.get()".to_string(),
            suggested_code: "let cached = self.balance.get()".to_string(),
            estimated_savings_ink: 1_200_000,
            estimated_savings_percentage: 50.0,
            confidence: "high".to_string(),
        };

        assert_eq!(opt.id, "cache_balance");
        assert_eq!(opt.estimated_savings_ink, 1_200_000);
        assert!(opt.estimated_savings_percentage > 0.0);
    }

    #[test]
    fn test_vscode_decorations_complete_structure() {
        let decorations = VsCodeDecorations {
            file: "src/lib.rs".to_string(),
            function: "transfer".to_string(),
            total_ink: 5_000_000,
            gas_equivalent: 500,
            decorations: Decorations {
                inline: vec![InlineDecoration {
                    line: 10,
                    text: "1.2M ink".to_string(),
                    color: "red".to_string(),
                }],
                gutter: vec![GutterDecoration {
                    line: 10,
                    icon: "flame".to_string(),
                    severity: "high".to_string(),
                }],
                hovers: vec![HoverDecoration {
                    line: 10,
                    markdown: "### Storage Read\n1.2M ink".to_string(),
                }],
                code_actions: vec![CodeAction {
                    line: 10,
                    title: "Cache this read".to_string(),
                    replacement: Replacement {
                        start_line: 10,
                        end_line: 10,
                        new_text: "let cached = ...".to_string(),
                    },
                }],
            },
        };

        assert_eq!(decorations.total_ink, 5_000_000);
        assert_eq!(decorations.decorations.inline.len(), 1);
        assert_eq!(decorations.decorations.gutter.len(), 1);
        assert_eq!(decorations.decorations.hovers.len(), 1);
        assert_eq!(decorations.decorations.code_actions.len(), 1);
    }

    #[test]
    fn test_function_analysis_with_all_fields() {
        let mut categories = HashMap::new();
        categories.insert(
            "storage".to_string(),
            CategoryStats {
                count: 3,
                total_ink: 3_000_000,
                percentage: 60.0,
                avg_per_op: 1_000_000,
            },
        );

        let func = FunctionAnalysis {
            name: "transfer".to_string(),
            signature: "transfer(to: Address, amount: U256)".to_string(),
            start_line: 3,
            total_ink: 5_000_000,
            gas_equivalent: 500,
            operations: vec![],
            categories,
            optimizations: vec![],
            hotspots: vec![],
            dry_nib_bugs: vec![],
        };

        assert_eq!(func.name, "transfer");
        assert_eq!(func.total_ink, 5_000_000);
        assert_eq!(func.gas_equivalent, 500);
        assert_eq!(func.categories.len(), 1);
    }

    #[test]
    fn test_json_round_trip_contract_analysis() {
        let mut functions = HashMap::new();
        functions.insert(
            "transfer".to_string(),
            FunctionAnalysis {
                name: "transfer".to_string(),
                signature: "transfer(...)".to_string(),
                start_line: 10,
                total_ink: 1_000_000,
                gas_equivalent: 100,
                operations: vec![],
                categories: HashMap::new(),
                optimizations: vec![],
                hotspots: vec![],
                dry_nib_bugs: vec![],
            },
        );

        let analysis = ContractAnalysis {
            contract_name: "ERC20".to_string(),
            file: "src/lib.rs".to_string(),
            functions,
        };

        let json = serde_json::to_string_pretty(&analysis).unwrap();
        let restored: ContractAnalysis = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.contract_name, "ERC20");
        assert_eq!(restored.functions.len(), 1);
        assert_eq!(restored.functions.get("transfer").unwrap().name, "transfer");
    }

    #[test]
    fn test_dry_nib_bug_structure() {
        let bug = DryNibBug {
            line: 42,
            operation: "msg::sender()".to_string(),
            category: "evm_context".to_string(),
            ink_charged_estimate: 300_000,
            actual_return_size: 20,
            buffer_allocated: 32,
            expected_fair_cost: 200_000,
            overcharge_estimate: 100_000,
            severity: "medium".to_string(),
            mitigation: "Cache the result".to_string(),
        };

        assert_eq!(bug.line, 42);
        assert_eq!(bug.actual_return_size, 20);
        assert_eq!(bug.buffer_allocated, 32);
        assert!(bug.overcharge_estimate > 0);
    }

    #[test]
    fn test_dry_nib_serialization() {
        let bug = DryNibBug {
            line: 10,
            operation: "storage_read".to_string(),
            category: "storage".to_string(),
            ink_charged_estimate: 1_400_000,
            actual_return_size: 32,
            buffer_allocated: 64,
            expected_fair_cost: 1_200_000,
            overcharge_estimate: 200_000,
            severity: "high".to_string(),
            mitigation: "Use batch reads".to_string(),
        };

        let json = serde_json::to_string(&bug).unwrap();
        let restored: DryNibBug = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.operation, "storage_read");
        assert_eq!(restored.overcharge_estimate, 200_000);
    }

    #[test]
    fn test_function_analysis_with_dry_nib_bugs() {
        let func = FunctionAnalysis {
            name: "transfer".to_string(),
            signature: "transfer(...)".to_string(),
            start_line: 76,
            total_ink: 5_000_000,
            gas_equivalent: 500,
            operations: vec![],
            categories: HashMap::new(),
            optimizations: vec![],
            hotspots: vec![],
            dry_nib_bugs: vec![DryNibBug {
                line: 10,
                operation: "msg::sender()".to_string(),
                category: "evm_context".to_string(),
                ink_charged_estimate: 300_000,
                actual_return_size: 20,
                buffer_allocated: 32,
                expected_fair_cost: 200_000,
                overcharge_estimate: 100_000,
                severity: "medium".to_string(),
                mitigation: "Cache result".to_string(),
            }],
        };

        assert_eq!(func.dry_nib_bugs.len(), 1);
        assert_eq!(func.dry_nib_bugs[0].overcharge_estimate, 100_000);
    }
}
