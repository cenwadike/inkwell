// src/analyzer.rs
use anyhow::{Context, Result};
use std::collections::HashMap;
use syn::BinOp;
use syn::{Expr, ExprMethodCall, ImplItem, ItemImpl, Stmt, parse_file, visit::Visit};
use std::path::PathBuf;
use crate::types::*;

/// Analyze a Rust contract and estimate ink consumption
pub fn analyze_contract(source: &str, target_function: Option<&str>, file_path_rel: PathBuf) -> Result<ContractAnalysis> {
    let ast = parse_file(source).context("Failed to parse Rust file")?;

    let mut visitor = ContractVisitor::new(target_function);
    visitor.visit_file(&ast);

    let contract_name = extract_contract_name(source);

    Ok(ContractAnalysis {
        contract_name,
        // Use the provided path dynamically
        file: file_path_rel.to_string_lossy().into_owned(),
        functions: visitor.functions,
    })
}

struct ContractVisitor {
    target_function: Option<String>,
    functions: Vec<FunctionAnalysis>,
    current_function: Option<String>,
    current_line: usize,
}

impl ContractVisitor {
    fn new(target_function: Option<&str>) -> Self {
        Self {
            target_function: target_function.map(|s| s.to_string()),
            functions: Vec::new(),
            current_function: None,
            current_line: 0,
        }
    }

    fn analyze_function(&mut self, name: String, signature: String, body: &[Stmt]) {
        // Skip if we're targeting a specific function and this isn't it
        if let Some(ref target) = self.target_function {
            if target != &name {
                return;
            }
        }

        let mut operations = Vec::new();
        let mut line_num = self.current_line;

        // Analyze each statement in the function body
        for stmt in body {
            line_num += 1;
            let op = self.analyze_statement(stmt, line_num);
            operations.extend(op);
        }

        // Calculate percentages
        let total_ink: u64 = operations.iter().map(|op| op.ink).sum();
        for op in &mut operations {
            op.percentage = if total_ink > 0 {
                (op.ink as f64 / total_ink as f64) * 100.0
            } else {
                0.0
            };
        }

        let gas_equivalent = total_ink / 10000; // 1 gas ≈ 10,000 ink

        // Calculate categories
        let categories = self.calculate_categories(&operations);

        // Detect optimizations (including dry nib bugs)
        let optimizations = self.detect_optimizations(&operations);

        // Detect dry nib bugs specifically
        let dry_nib_bugs = self.detect_dry_nib_bugs(&operations);

        // Identify hotspots (operations > 1M ink)
        let mut hotspots: Vec<Hotspot> = operations
            .iter()
            .filter(|op| op.ink > 1_000_000)
            .enumerate()
            .map(|(idx, op)| Hotspot {
                line: op.line,
                ink: op.ink,
                operation: op.operation.clone(),
                rank: idx + 1,
            })
            .collect();

        // Sort by ink descending
        hotspots.sort_by(|a, b| b.ink.cmp(&a.ink));
        for (idx, hotspot) in hotspots.iter_mut().enumerate() {
            hotspot.rank = idx + 1;
        }

        self.functions.push(FunctionAnalysis {
            name,
            signature,
            total_ink,
            gas_equivalent,
            operations,
            categories,
            optimizations,
            hotspots,
            dry_nib_bugs,
        });
    }

    fn detect_dry_nib_bugs(&self, operations: &[Operation]) -> Vec<DryNibBug> {
        let mut bugs = Vec::new();

        // Group reads by storage field for repetition detection
        let mut read_counts: HashMap<String, Vec<usize>> = HashMap::new();
        let mut nested_accesses = Vec::new();

        for op in operations {
            if op.category != "storage_read" && op.category != "storage_write" {
                continue;
            }

            let var = extract_storage_variable(&op.code);
            if var == "unknown" || var.is_empty() {
                continue;
            }

            if op.category == "storage_read" {
                read_counts.entry(var.clone()).or_default().push(op.line);

                // Detect nested map access
                let get_occurrences = op.code.matches(".get(").count();
                if get_occurrences >= 2 {
                    nested_accesses.push((op.line, op.operation.clone(), var.clone()));
                }
            }

            // Always run the original per-op estimation as fallback
            let (base_cost, estimated_return_size) = match op.category.as_str() {
                "storage_read" => {
                    if op.code.matches(".get(").count() >= 2 {
                        (800_000, 32) // nested
                    } else {
                        (800_000, 32)
                    }
                }
                "storage_write" => (1_000_000, 32),
                _ => continue,
            };

            let fair_overhead = base_cost + (estimated_return_size as u64 * 100);
            let buffer_size = Self::estimate_buffer_allocation(estimated_return_size);
            let words = (buffer_size + 31) / 32;
            let likely_charged = base_cost + (words as u64 * 1000);
            let overcharge = likely_charged.saturating_sub(fair_overhead);

            // Much more aggressive threshold for storage ops
            if overcharge > 150_000 || op.code.matches(".get(").count() >= 2 {
                bugs.push(DryNibBug {
                    line: op.line,
                    operation: op.operation.clone(),
                    category: op.category.clone(),
                    ink_charged_estimate: likely_charged,
                    actual_return_size: estimated_return_size,
                    buffer_allocated: buffer_size,
                    expected_fair_cost: fair_overhead,
                    overcharge_estimate: overcharge,
                    severity: if overcharge > 800_000 {
                        "high".to_string()
                    } else {
                        "medium".to_string()
                    },
                    mitigation: Self::suggest_dry_nib_mitigation(&op.operation),
                });
            }
        }

        // === Repeated reads = cumulative dry nib waste ===
        for (field, lines) in read_counts {
            if lines.len() >= 3 {
                let wasted_ink = (lines.len() as u64 - 1) * 1_200_000;
                bugs.push(DryNibBug {
                    line: lines[0],
                    operation: format!("repeated storage_read: self.{}", field),
                    category: "storage_read".to_string(),
                    ink_charged_estimate: lines.len() as u64 * 1_200_000,
                    actual_return_size: 32,
                    buffer_allocated: 64,
                    expected_fair_cost: 1_200_000,
                    overcharge_estimate: wasted_ink,
                    severity: "high".to_string(),
                    mitigation: format!(
                        "Cache `self.{}` in a local variable. Repeated host calls are a major dry-nib source (each .get() incurs buffer allocation overhead).",
                        field
                    ),
                });
            }
        }

        // === Nested map accesses ===
        for (line, op, field) in nested_accesses {
            // Stylus Base Costs:
            // Host I/O transition overhead is ~0.84 gas (approx 840,000 ink units).
            // Each storage read (SLOAD) is 2100 gas (cold) or 100 gas (warm).
            
            let estimated_ink_per_gas = 1_000_000; // 1 Gas = 1,000,000 Ink units in Stylus
            let base_host_io_ink = 840_000;         // Cost to suspend WASM and call host
            
            // For nested maps, we do 2 storage reads. 
            // Stylus SDK cache might make the 2nd one 'warm', but the host call overhead remains.
            let estimated_ink = (base_host_io_ink * 2) + (2100 * estimated_ink_per_gas);
            let fair_ink = base_host_io_ink + (100 * estimated_ink_per_gas); // Ideal cached cost
            
            let estimated_return_size = 32; // Standard 32-byte slot
            let buffer_size = 64; // Stylus uses 32 bytes for the key + 32 for the value

            bugs.push(DryNibBug {
                line,
                operation: op,
                category: "storage_read".to_string(),
                ink_charged_estimate: estimated_ink,
                actual_return_size: estimated_return_size,
                buffer_allocated: buffer_size, 
                expected_fair_cost: fair_ink,
                overcharge_estimate: estimated_ink.saturating_sub(fair_ink),
                severity: "high".to_string(),
                mitigation: format!(
                    "Nested access on storage field `{}` detected. In Arbitrum Stylus, this triggers multiple host I/O calls. \
                    Use `self.{}.getter(key)` to cache the intermediate mapping and avoid redundant WASM-to-Host transitions.",
                    field, field
                ),
            });
        }

        bugs
    }

    fn estimate_buffer_allocation(actual_size: usize) -> usize {
        // Stylus often allocates in powers of 2 or fixed sizes
        match actual_size {
            0..=8 => 32,                         // Even small values get 32-byte buffer
            9..=32 => 64,                        // 32-byte values often get 64-byte buffer
            33..=64 => 128,                      // And so on...
            _ => ((actual_size + 63) / 64) * 64, // Round up to nearest 64
        }
    }

    fn suggest_dry_nib_mitigation(operation: &str) -> String {
        if operation.contains("storage_read") || operation.contains("storage_write") {
            "Storage operations have buffer overhead. Cache repeated reads in local variables. \
             For nested maps like self.balances.get(addr), the outer .get() call allocates a buffer \
             even though it just returns a storage pointer. Consider restructuring to minimize map nesting depth."
                .to_string()
        } else if operation.contains("msg::sender") {
            "Cache msg::sender() result in a local variable if used multiple times. \
             The 20-byte address is often charged for 32+ bytes of overhead."
                .to_string()
        } else if operation.contains("block::") {
            "Cache block properties in local variables. Even small values like block.number (u64) \
             may be charged for full 32-byte word overhead."
                .to_string()
        } else {
            "Minimize host calls by batching operations and caching results where possible."
                .to_string()
        }
    }

    fn analyze_statement(&self, stmt: &Stmt, line: usize) -> Vec<Operation> {
        let mut ops = Vec::new();

        match stmt {
            Stmt::Local(local) => {
                if let Some(init) = &local.init {
                    ops.extend(self.analyze_expr(&init.expr, line));
                }
            }
            Stmt::Expr(expr, _) => {
                ops.extend(self.analyze_expr(expr, line));
            }
            Stmt::Macro(mac) => {
                let mac_str = quote::quote!(#mac).to_string();
                // Detect common macros
                if mac_str.contains("require!") || mac_str.contains("assert!") {
                    ops.push(Operation {
                        line,
                        column: 0,
                        code: mac_str.clone(),
                        operation: "require_check".to_string(),
                        ink: 50_000,
                        percentage: 0.0,
                        category: "control_flow".to_string(),
                        severity: "low".to_string(),
                    });
                }
            }
            _ => {}
        }

        ops
    }

    fn analyze_expr(&self, expr: &Expr, line: usize) -> Vec<Operation> {
        let mut ops = Vec::new();
        let expr_str = quote::quote!(#expr).to_string().trim().to_string();

        // Storage READ detection
        if let Some(read_type) = self.detect_storage_read(&expr_str) {
            ops.push(self.build_operation(
                line,
                expr_str.clone(),
                format!("storage_read ({})", read_type),
                "storage_read".to_string(),
                "high".to_string(),
            ));
        }

        // Storage WRITE detection
        if let Some(write_type) = self.detect_storage_write(&expr_str) {
            let has_embedded_read = self.has_embedded_read(&expr_str);
            let category = "storage_write".to_string();
            let severity = "high".to_string();
            let extra = if has_embedded_read {
                " + embedded_read"
            } else {
                ""
            }
            .to_string();

            ops.push(self.build_operation(
                line,
                expr_str.clone(),
                format!("storage_write ({}{})", write_type, extra),
                category,
                severity,
            ));
        }

        // Host call detections
        if expr_str.contains("msg::sender()") || expr_str.contains("msg.sender()") {
            ops.push(self.build_operation(
                line,
                expr_str.clone(),
                "msg::sender()".to_string(),
                "evm_context".to_string(),
                "low".to_string(),
            ));
        }

        if expr_str.contains("msg::value()") || expr_str.contains("msg.value()") {
            ops.push(self.build_operation(
                line,
                expr_str.clone(),
                "msg::value()".to_string(),
                "evm_context".to_string(),
                "low".to_string(),
            ));
        }

        if expr_str.contains("block::number()")
            || expr_str.contains("block.number()")
            || expr_str.contains("block::timestamp()")
            || expr_str.contains("block.timestamp()")
        {
            ops.push(self.build_operation(
                line,
                expr_str.clone(),
                "block_info".to_string(),
                "evm_context".to_string(),
                "low".to_string(),
            ));
        }

        if expr_str.contains("evm::log(") || expr_str.contains(".emit(") {
            ops.push(self.build_operation(
                line,
                expr_str.clone(),
                "event_emit".to_string(),
                "event".to_string(),
                "medium".to_string(),
            ));
        }

        if expr_str.contains(".call(") || expr_str.contains("CallBuilder") {
            ops.push(self.build_operation(
                line,
                expr_str.clone(),
                "external_call".to_string(),
                "external_call".to_string(),
                "high".to_string(),
            ));
        }

        if expr_str.contains("keccak256")
            || expr_str.contains("sha256")
            || expr_str.contains("ecdsa")
        {
            ops.push(self.build_operation(
                line,
                expr_str.clone(),
                "crypto_hash".to_string(),
                "crypto".to_string(),
                "medium".to_string(),
            ));
        }

        // Recursive analysis
        match expr {
            Expr::MethodCall(method) => {
                ops.extend(self.analyze_method_call(method, line));
            }

            Expr::Call(call) => {
                ops.extend(self.analyze_expr(&call.func, line));
                for arg in &call.args {
                    ops.extend(self.analyze_expr(arg, line));
                }
            }

            Expr::Binary(bin) => {
                ops.extend(self.analyze_expr(&bin.left, line));
                ops.extend(self.analyze_expr(&bin.right, line));

                if matches!(
                    bin.op,
                    BinOp::AddAssign(_)
                        | BinOp::SubAssign(_)
                        | BinOp::MulAssign(_)
                        | BinOp::DivAssign(_)
                ) {
                    if self.looks_like_storage_write(&bin.left) {
                        ops.push(self.build_operation(
                            line,
                            expr_str.clone(),
                            "storage_compound_update".to_string(),
                            "storage_write".to_string(),
                            "high".to_string(),
                        ));
                    }
                }
            }

            Expr::Assign(assign) => {
                ops.extend(self.analyze_expr(&assign.left, line));
                ops.extend(self.analyze_expr(&assign.right, line));

                if self.looks_like_storage_write(&assign.left) {
                    ops.push(self.build_operation(
                        line,
                        expr_str.clone(),
                        "storage_assign".to_string(),
                        "storage_write".to_string(),
                        "high".to_string(),
                    ));
                }
            }

            Expr::Index(index) => {
                ops.extend(self.analyze_expr(&index.expr, line));
                ops.extend(self.analyze_expr(&index.index, line));

                if self.looks_like_storage_access(&index.expr) {
                    ops.push(self.build_operation(
                        line,
                        expr_str.clone(),
                        "storage_index_access".to_string(),
                        "storage_read".to_string(),
                        "high".to_string(),
                    ));
                }
            }

            Expr::Field(field) => {
                ops.extend(self.analyze_expr(&field.base, line));

                if self.looks_like_storage_access(&field.base) {
                    ops.push(self.build_operation(
                        line,
                        expr_str.clone(),
                        "storage_field_access".to_string(),
                        "storage_read".to_string(),
                        "high".to_string(),
                    ));
                }
            }

            _ => {}
        }

        ops
    }

    fn build_operation(
        &self,
        line: usize,
        code: String,
        operation: String,
        category: String,
        severity: String,
    ) -> Operation {
        let ink = self.estimate_ink_cost(&operation, &category);

        Operation {
            line,
            column: 0,
            code,
            operation,
            ink,
            percentage: 0.0,
            category,
            severity,
        }
    }

    fn estimate_ink_cost(&self, operation: &str, category: &str) -> u64 {
        match category {
            "storage_read" => 1_200_000,
            "storage_write" => {
                if operation.contains("embedded_read") {
                    2_400_000
                } else {
                    1_500_000
                }
            }
            "evm_context" => {
                // Host calls often have hidden overhead from buffer allocation
                if operation.contains("msg::sender") {
                    300_000 // 20 bytes actual, but charged for 32+ byte buffer
                } else if operation.contains("msg::value") {
                    350_000 // 32 bytes but often padded
                } else if operation.contains("block::") {
                    250_000 // Small values but 32-byte buffer overhead
                } else {
                    200_000
                }
            }
            "event" => 350_000,
            "external_call" => 2_500_000,
            "crypto" => 500_000,
            "assignment" => 80_000,
            _ => 50_000,
        }
    }

    fn looks_like_storage_write(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Path(path) => self.path_contains_storage(&path.path),
            Expr::Field(field) => self.looks_like_storage_write(&field.base),
            Expr::Index(index) => self.looks_like_storage_write(&index.expr),
            _ => false,
        }
    }

    fn looks_like_storage_access(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Path(path) => self.path_starts_with_self(&path.path),
            Expr::Field(field) => {
                self.looks_like_storage_access(&field.base)
                    || matches!(&field.member, syn::Member::Named(ident) if ident == "storage")
            }
            Expr::Index(index) => self.looks_like_storage_access(&index.expr),
            _ => false,
        }
    }

    fn path_starts_with_self(&self, path: &syn::Path) -> bool {
        path.segments
            .first()
            .map_or(false, |seg| seg.ident == "self")
    }

    fn path_contains_storage(&self, path: &syn::Path) -> bool {
        path.segments.iter().any(|seg| {
            let name = seg.ident.to_string();
            name.contains("storage")
                || name.contains("balances")
                || name.contains("map")
                || name.contains("vec")
        })
    }

    fn analyze_method_call(&self, method: &ExprMethodCall, line: usize) -> Vec<Operation> {
        let mut ops = Vec::new();

        if !matches!(&*method.receiver, Expr::MethodCall(_)) {
            ops.extend(self.analyze_expr(&method.receiver, line));
        }

        for arg in &method.args {
            ops.extend(self.analyze_expr(arg, line));
        }

        ops
    }

    fn detect_storage_read(&self, expr: &str) -> Option<&'static str> {
        let normalized = expr
            .replace(" . ", ".")
            .replace(". ", ".")
            .replace(" (", "(");

        if !normalized.contains("self.") {
            return None;
        }

        if normalized.contains(".get(")
            || normalized.contains(".getter()")
            || normalized.contains(".at(")
            || normalized.contains(".value()")
            || normalized.contains(".len()")
        {
            return Some("get()");
        }

        if normalized.starts_with("self.") && !normalized.contains('(') && !normalized.contains('=')
        {
            return Some("direct");
        }

        None
    }

    fn detect_storage_write(&self, expr: &str) -> Option<&'static str> {
        let normalized = expr
            .replace(" . ", ".")
            .replace(". ", ".")
            .replace(" (", "(");

        if !normalized.contains("self.") {
            return None;
        }

        if normalized.contains(".insert(")
            || normalized.contains(".set(")
            || normalized.contains(".push(")
        {
            return Some("write()");
        }

        if normalized.contains("+=") || normalized.contains("-=") {
            return Some("compound_write");
        }

        None
    }

    fn has_embedded_read(&self, expr: &str) -> bool {
        let has_write = expr.contains(".insert(") || expr.contains(".set(");
        let has_read = expr.contains(".get(") || expr.contains(".getter(");

        has_write && has_read
    }

    fn calculate_categories(&self, operations: &[Operation]) -> HashMap<String, CategoryStats> {
        let mut categories: HashMap<String, Vec<u64>> = HashMap::new();

        for op in operations {
            categories
                .entry(op.category.clone())
                .or_default()
                .push(op.ink);
        }

        let total_ink: u64 = operations.iter().map(|op| op.ink).sum();

        categories
            .into_iter()
            .map(|(category, inks)| {
                let total: u64 = inks.iter().sum();
                let count = inks.len();
                let avg = if count > 0 { total / count as u64 } else { 0 };
                let percentage = if total_ink > 0 {
                    (total as f64 / total_ink as f64) * 100.0
                } else {
                    0.0
                };

                (
                    category,
                    CategoryStats {
                        count,
                        total_ink: total,
                        percentage,
                        avg_per_op: avg,
                    },
                )
            })
            .collect()
    }

    fn detect_optimizations(&self, operations: &[Operation]) -> Vec<Optimization> {
        let mut optimizations = Vec::new();

        // Detect redundant storage reads in write operations
        for (idx, op) in operations.iter().enumerate() {
            if op.category == "storage_write" && op.ink > 2_000_000 {
                optimizations.push(Optimization {
                    id: format!("redundant_read_{}", idx),
                    line: op.line,
                    severity: "high".to_string(),
                    title: "Redundant Storage Read in Write".to_string(),
                    description: "This write operation contains an embedded storage read. Separate the read into a local variable to save ~1.2M ink.".to_string(),
                    current_code: truncate_code(&op.code, 80),
                    suggested_code: "// Separate read and write:\n// let cached = storage.get(key);\n// storage.set(key, cached + value);".to_string(),
                    estimated_savings_ink: 1_200_000,
                    estimated_savings_percentage: 50.0,
                    confidence: "high".to_string(),
                });
            }
        }

        // Detect repeated storage reads
        let mut read_map: HashMap<String, Vec<usize>> = HashMap::new();
        for op in operations {
            if op.category == "storage_read" {
                let var = extract_storage_variable(&op.code);
                read_map.entry(var).or_default().push(op.line);
            }
        }

        for (var, lines) in read_map {
            if lines.len() > 1 && var != "unknown" && !var.is_empty() {
                optimizations.push(Optimization {
                    id: format!("cache_{}", var),
                    line: lines[0],
                    severity: "medium".to_string(),
                    title: format!("Cache repeated storage read: self.{}", var),
                    description: format!(
                        "Field `{}` is read {}× → cache in local variable → save ~{:.1}M ink",
                        var,
                        lines.len(),
                        (lines.len() - 1) as f64 * 1.2
                    ),
                    current_code: format!("// Reads at lines: {:?}", lines),
                    suggested_code: format!(
                        "let cached_{} = self.{}.get(...);\n// Use cached_{} instead",
                        var, var, var
                    ),
                    estimated_savings_ink: 1_200_000 * (lines.len() as u64 - 1),
                    estimated_savings_percentage: ((lines.len() - 1) as f64 / lines.len() as f64)
                        * 100.0,
                    confidence: "high".to_string(),
                });
            }
        }

        optimizations
    }
}

impl<'ast> Visit<'ast> for ContractVisitor {
    fn visit_item_impl(&mut self, node: &'ast ItemImpl) {
        let is_external = node
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("external"));

        if is_external {
            for item in &node.items {
                if let ImplItem::Fn(method) = item {
                    let name = method.sig.ident.to_string();
                    let signature = format!("{}(...)", name);

                    self.current_function = Some(name.clone());
                    self.current_line = 0;

                    let body = &method.block.stmts;
                    self.analyze_function(name, signature, body);
                }
            }
        }

        syn::visit::visit_item_impl(self, node);
    }
}

fn extract_contract_name(source: &str) -> String {
    if let Some(pos) = source.find("pub struct") {
        let after = &source[pos..];
        if let Some(end) = after.find('{') {
            let declaration = &after[..end];
            if let Some(name) = declaration.split_whitespace().nth(2) {
                return name.to_string();
            }
        }
    }

    "Unknown".to_string()
}

fn extract_storage_variable(code: &str) -> String {
    let code_normalized = code.trim().replace(" ", "");
    let re_outer = regex::Regex::new(r"self\.([a-zA-Z_][a-zA-Z0-9_]*)\.").unwrap();

    if let Some(caps) = re_outer.captures(&code_normalized) {
        if let Some(m) = caps.get(1) {
            let name = m.as_str().to_string();
            if !["mut", "ref", "as", "let", "where", "self", "get", "insert"]
                .contains(&name.as_str())
            {
                return name;
            }
        }
    }

    "unknown".to_string()
}

fn truncate_code(code: &str, max_len: usize) -> String {
    if code.len() <= max_len {
        code.to_string()
    } else {
        format!("{}...", &code[..max_len - 3])
    }
}
