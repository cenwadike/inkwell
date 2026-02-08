// src/types.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractAnalysis {
    pub contract_name: String,
    pub file: String,
    pub functions: Vec<FunctionAnalysis>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionAnalysis {
    pub name: String,
    pub signature: String,
    pub total_ink: u64,
    pub gas_equivalent: u64,
    pub operations: Vec<Operation>,
    pub categories: HashMap<String, CategoryStats>,
    pub optimizations: Vec<Optimization>,
    pub hotspots: Vec<Hotspot>,
    pub dry_nib_bugs: Vec<DryNibBug>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub line: usize,
    pub column: usize,
    pub code: String,
    pub operation: String,
    pub ink: u64,
    pub percentage: f64,
    pub category: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryStats {
    pub count: usize,
    pub total_ink: u64,
    pub percentage: f64,
    pub avg_per_op: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Optimization {
    pub id: String,
    pub line: usize,
    pub severity: String,
    pub title: String,
    pub description: String,
    pub current_code: String,
    pub suggested_code: String,
    pub estimated_savings_ink: u64,
    pub estimated_savings_percentage: f64,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hotspot {
    pub line: usize,
    pub ink: u64,
    pub operation: String,
    pub rank: usize,
}

/// Represents a "dry nib" bug: host-call overhead charges for more data than actually returned
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DryNibBug {
    pub line: usize,
    pub operation: String,
    pub category: String,
    /// Estimated ink charged by Stylus (including buffer overhead)
    pub ink_charged_estimate: u64,
    /// Actual size of data returned (in bytes)
    pub actual_return_size: usize,
    /// Size of buffer allocated by Stylus (often larger than actual)
    pub buffer_allocated: usize,
    /// Fair cost based on actual return size
    pub expected_fair_cost: u64,
    /// Estimated overcharge amount
    pub overcharge_estimate: u64,
    /// Severity: high, medium, low
    pub severity: String,
    /// Suggested mitigation
    pub mitigation: String,
}

// VS Code decoration types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsCodeDecorations {
    pub file: String,
    pub function: String,
    pub total_ink: u64,
    pub gas_equivalent: u64,
    pub decorations: Decorations,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decorations {
    pub inline: Vec<InlineDecoration>,
    pub gutter: Vec<GutterDecoration>,
    pub hovers: Vec<HoverDecoration>,
    pub code_actions: Vec<CodeAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InlineDecoration {
    pub line: usize,
    pub text: String,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GutterDecoration {
    pub line: usize,
    pub icon: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoverDecoration {
    pub line: usize,
    pub markdown: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeAction {
    pub line: usize,
    pub title: String,
    pub replacement: Replacement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replacement {
    pub start_line: usize,
    pub end_line: usize,
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
            functions: vec![],
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
        let analysis = ContractAnalysis {
            contract_name: "ERC20".to_string(),
            file: "src/lib.rs".to_string(),
            functions: vec![FunctionAnalysis {
                name: "transfer".to_string(),
                signature: "transfer(...)".to_string(),
                total_ink: 1_000_000,
                gas_equivalent: 100,
                operations: vec![],
                categories: HashMap::new(),
                optimizations: vec![],
                hotspots: vec![],
                dry_nib_bugs: vec![],
            }],
        };

        let json = serde_json::to_string_pretty(&analysis).unwrap();
        let restored: ContractAnalysis = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.contract_name, "ERC20");
        assert_eq!(restored.functions.len(), 1);
        assert_eq!(restored.functions[0].name, "transfer");
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
