use crate::types::*;
use anyhow::{Context, Result};
use quote::quote;
use std::collections::HashMap;
use std::path::PathBuf;
use syn::{BinOp, Expr, ExprMethodCall, ImplItem, ItemConst, ItemImpl, Stmt, visit::Visit};

/// Analyze a Stylus / Arbitrum smart contract written in Rust.
///
/// Parses the source code, identifies public/external entry points, traverses the AST,
/// collects storage-heavy operations, estimates ink consumption, detects repeated reads,
/// flags potential "dry nib" overcharge bugs, and suggests caching optimizations.
///
/// # Arguments
/// * `source`          - Complete source code of the contract file as a string
/// * `target_function` - Optional: analyze only one specific function by name
/// * `file_path_rel`   - Relative file path (mainly used in error messages)
///
/// # Returns
/// `Ok(ContractAnalysis)` containing contract name, file path and per-function metrics
///
/// # Errors
/// Returns `Err` if:
/// - source cannot be parsed as valid Rust syntax
/// - no eligible public/external functions are found (with diagnostic hints)
pub fn analyze_contract(
    source: &str,
    target_function: Option<&str>,
    file_path_rel: PathBuf,
) -> Result<ContractAnalysis> {
    let ast = syn::parse_file(source).context("Failed to parse Rust file")?;

    let function_lines = find_function_lines(source);

    let mut visitor = ContractVisitor::new(target_function, function_lines, source);
    visitor.visit_file(&ast);

    if visitor.functions.is_empty() {
        anyhow::bail!(
            "No public/external functions detected.\n\
             Possible causes:\n\
             • No #[public]/#[external] attributes found (classic Stylus style)\n\
             • Contract uses sol! macro (no attributes, dispatch via Router + SELECTOR_*)\n\
             • Methods are private or internal helpers only\n\
             File: {}\n\
             Selectors found: {} (if > 0 → likely sol! style)",
            file_path_rel.display(),
            visitor.selector_count
        );
    }

    Ok(ContractAnalysis {
        contract_name: extract_contract_name(source),
        file: file_path_rel.to_string_lossy().into_owned(),
        functions: visitor.functions,
    })
}

/// Scans source lines to approximate function starting line numbers.
///
/// Only looks for `fn` or `pub fn` at the beginning of trimmed lines.
/// Used to improve accuracy of reported line numbers in analysis output.
///
/// # Returns
/// `HashMap<function_name, 1-based_line_number>`
fn find_function_lines(source: &str) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("pub fn ") || trimmed.starts_with("fn ") {
            let start = if trimmed.starts_with("pub fn ") { 7 } else { 3 };
            if let Some(end) = trimmed[start..].find('(') {
                let name = trimmed[start..start + end].trim();
                if !name.is_empty() {
                    map.insert(name.to_string(), idx + 1);
                }
            }
        }
    }
    map
}

/// AST visitor responsible for identifying contract implementation blocks
/// and analyzing public/external methods for ink cost and optimization potential.
struct ContractVisitor<'a> {
    /// Optional single-function analysis mode
    target_function: Option<String>,
    /// Accumulated analysis results per function
    functions: HashMap<String, FunctionAnalysis>,
    /// Approximate starting line numbers of functions
    function_lines: HashMap<String, usize>,
    /// Whether we found something that looks like a selector-based router/dispatch
    has_router_impl: bool,
    /// Count of detected `SELECTOR_*` / `*_SELECTOR` constants
    selector_count: usize,
    /// Original source text (used for line number heuristics)
    source_text: &'a str,
}

impl<'a> ContractVisitor<'a> {
    /// Construct a new analysis visitor
    fn new(target: Option<&str>, lines: HashMap<String, usize>, source: &'a str) -> Self {
        Self {
            target_function: target.map(|s| s.to_string()),
            functions: HashMap::new(),
            function_lines: lines,
            has_router_impl: false,
            selector_count: 0,
            source_text: source,
        }
    }

    /// Best-effort search for the source line that most closely matches `search_text`
    /// within a ~±30 line window around `start_line`.
    ///
    /// Uses several fallback strategies: normalized substring match, storage access
    /// pattern preference, field name lookup, and finally the function start line.
    fn find_exact_line(&self, search_text: &str, start_line: usize) -> usize {
        let lines: Vec<&str> = self.source_text.lines().collect();

        let norm_search = search_text
            .replace(char::is_whitespace, " ")
            .trim()
            .to_string();

        if norm_search.is_empty() {
            return start_line;
        }

        let search_start = start_line.saturating_sub(30).max(1) - 1;
        let search_end = (start_line + 60).min(lines.len());

        for (idx, line) in lines
            .iter()
            .enumerate()
            .skip(search_start)
            .take(search_end - search_start)
        {
            let replaced = line.replace(char::is_whitespace, " ");
            let norm_line = replaced.trim();

            if norm_line.contains(&norm_search) || norm_search.contains(norm_line) {
                if (norm_search.contains(".get(") || norm_search.contains(".set("))
                    && (line.contains(".get(") || line.contains(".set("))
                {
                    return idx + 1;
                }
                if norm_search.contains("ZERO") && line.contains("ZERO") {
                    return idx + 1;
                }
                return idx + 1;
            }
        }

        let field = extract_storage_variable(search_text);
        if field != "unknown" {
            for (idx, line) in lines
                .iter()
                .enumerate()
                .skip(search_start)
                .take(search_end - search_start)
            {
                if line.contains(&field) && (line.contains(".get(") || line.contains(".set(")) {
                    return idx + 1;
                }
            }
        }

        start_line
    }

    /// Core function analysis logic: collect operations, compute ink, percentages,
    /// categories, hotspots, dry-nib bugs and optimization suggestions.
    ///
    /// Skips known internal / infrastructure methods.
    fn analyze_function(
        &mut self,
        name: String,
        signature: String,
        body: &[Stmt],
        fn_start_line: usize,
    ) {
        if let Some(ref t) = self.target_function
            && t != &name
        {
            return;
        }

        // Skip internal helpers that are not part of the public ABI
        if name == "required_slots"
            || name.starts_with("__stylus_")
            || name == "new"
            || name == "load"
            || name == "load_mut"
            || name == "entrypoint"
            || name == "user_entrypoint"
            || name == "mark_used"
        {
            return;
        }

        let mut operations = Vec::new();

        for stmt in body {
            let ops = self.analyze_statement(stmt, fn_start_line);
            operations.extend(ops);
        }

        let total_ink = self.compute_total_ink(&operations);

        for op in &mut operations {
            op.percentage = if total_ink > 0 {
                (op.ink as f64 / total_ink as f64) * 100.0
            } else {
                0.0
            };
        }

        let gas_equivalent = total_ink / 10000;

        let categories = self.calculate_categories(&operations);
        let optimizations = self.detect_optimizations(&operations);
        let dry_nib_bugs = self.detect_dry_nib_bugs(&operations);

        let mut hotspots: Vec<Hotspot> = operations
            .iter()
            .filter(|op| op.ink > 1_000_000)
            .enumerate()
            .map(|(i, op)| Hotspot {
                line: op.line,
                ink: op.ink,
                operation: op.operation.clone(),
                rank: i + 1,
            })
            .collect();

        hotspots.sort_by(|a, b| b.ink.cmp(&a.ink));
        for (i, h) in hotspots.iter_mut().enumerate() {
            h.rank = i + 1;
        }

        let analysis = FunctionAnalysis {
            name: name.clone(),
            signature,
            start_line: fn_start_line,
            total_ink,
            gas_equivalent,
            operations,
            categories,
            optimizations,
            hotspots,
            dry_nib_bugs,
        };

        self.functions.insert(name, analysis);
    }

    /// Calculate total estimated ink, including extra penalty for every storage operation
    /// (approximating field access + load cost added by the Stylus environment).
    fn compute_total_ink(&self, ops: &[Operation]) -> u64 {
        let mut total = 0u64;
        for op in ops {
            total += op.ink;
            // Terminal adds extra cost for field access + load per storage op
            if op.category == "storage_read" || op.category == "storage_write" {
                total += 2_400_000; // load + field_access
            }
        }
        total
    }

    /// Detect patterns that are likely overcharged due to unnecessary buffer allocation
    /// or nested `.get()` calls (classic "dry nib" issue in Stylus storage access).
    fn detect_dry_nib_bugs(&self, ops: &[Operation]) -> Vec<DryNibBug> {
        let mut bugs = vec![];

        for op in ops {
            if op.category != "storage_read" {
                continue;
            }

            let get_count = op.code.matches(".get(").count();

            let suspected_overcharge = get_count >= 2
                || op.code.contains("balances")
                || op.code.contains("allowance")
                || op.ink >= 3_000_000;

            if suspected_overcharge {
                let charged = if get_count >= 2 {
                    4_800_000u64
                } else {
                    2_400_000u64
                };
                let over = charged.saturating_sub(1_200_000);

                bugs.push(DryNibBug {
                    line: op.line,
                    operation: op.operation.clone(),
                    category: "storage_read".to_string(),
                    ink_charged_estimate: charged,
                    actual_return_size: 32,
                    buffer_allocated: 64,
                    expected_fair_cost: 1_200_000,
                    overcharge_estimate: over,
                    severity: if over > 2_000_000 {
                        "high".to_string()
                    } else {
                        "medium".to_string()
                    },
                    mitigation: if get_count >= 2 {
                        "Cache outer mapping result before inner .get()".to_string()
                    } else {
                        "Cache storage value in local variable".to_string()
                    },
                });
            }
        }

        bugs.sort_by_key(|b| b.line);
        bugs
    }

    /// Recursively analyze different kinds of statements looking for expensive operations.
    fn analyze_statement(&self, stmt: &Stmt, fn_start_line: usize) -> Vec<Operation> {
        let mut ops = Vec::new();

        match stmt {
            Stmt::Local(l) => {
                if let Some(init) = &l.init {
                    let expr_str = quote!(#(init.expr)).to_string();
                    let actual_line = self.find_exact_line(&expr_str, fn_start_line);
                    ops.extend(self.analyze_expr(&init.expr, actual_line));
                }
            }
            Stmt::Expr(e, _) => {
                let expr_str = quote!(#e).to_string();
                let actual_line = self.find_exact_line(&expr_str, fn_start_line);
                ops.extend(self.analyze_expr(e, actual_line));
            }
            Stmt::Macro(m) => {
                let s = quote!(#m).to_string();
                if s.contains("require!") || s.contains("assert!") {
                    ops.push(Operation {
                        line: fn_start_line + 1,
                        column: 0,
                        code: s,
                        operation: "require_check".to_string(),
                        entity: "n/a".to_string(),
                        ink: 50000,
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

    /// Recursively walk an expression tree looking for storage accesses, EVM context calls,
    /// events, compound assignments that may imply writes, etc.
    fn analyze_expr(&self, expr: &Expr, line: usize) -> Vec<Operation> {
        let mut ops = Vec::new();
        let expr_str = quote!(#expr).to_string().trim().to_string();

        // 1. Detect map.get / nested
        if let Some((field, is_nested)) = self.detect_storage_read(&expr_str) {
            let op_name = if is_nested {
                "nested_map_get"
            } else {
                "map::get"
            };
            ops.push(Operation {
                line,
                column: 0,
                code: expr_str.clone(),
                operation: op_name.to_string(),
                entity: field.clone(),
                ink: 1200000,
                percentage: 0.0,
                category: "storage_read".to_string(),
                severity: "high".to_string(),
            });

            ops.push(Operation {
                line,
                column: 0,
                code: expr_str.clone(),
                operation: "storage::load".to_string(),
                entity: field.clone(),
                ink: 1200000,
                percentage: 0.0,
                category: "storage_read".to_string(),
                severity: "high".to_string(),
            });

            ops.push(Operation {
                line,
                column: 0,
                code: format!("self.{}", field),
                operation: "storage_field_access".to_string(),
                entity: field,
                ink: 1200000,
                percentage: 0.0,
                category: "storage_read".to_string(),
                severity: "high".to_string(),
            });
        }

        // 2. Writes (with possible read-before-write)
        if self.detect_storage_write(&expr_str).is_some() {
            let entity = self.extract_storage_entity(&expr_str);
            let op_name = if expr_str.contains(".get(") {
                "map::upsert"
            } else {
                "map::insert"
            };

            ops.push(Operation {
                line,
                column: 0,
                code: expr_str.clone(),
                operation: op_name.to_string(),
                entity: entity.clone(),
                ink: 1500000,
                percentage: 0.0,
                category: "storage_write".to_string(),
                severity: "high".to_string(),
            });

            if expr_str.contains(".get(") {
                ops.push(Operation {
                    line,
                    column: 0,
                    code: expr_str.clone(),
                    operation: "storage::load".to_string(),
                    entity: entity.clone(),
                    ink: 1200000,
                    percentage: 0.0,
                    category: "storage_read".to_string(),
                    severity: "high".to_string(),
                });
                ops.push(Operation {
                    line,
                    column: 0,
                    code: format!("self.{}", entity),
                    operation: "storage_field_access".to_string(),
                    entity,
                    ink: 1200000,
                    percentage: 0.0,
                    category: "storage_read".to_string(),
                    severity: "high".to_string(),
                });
            }
        }

        // 3. Other calls
        if expr_str.contains("msg::sender()") {
            ops.push(self.build_operation(
                line,
                expr_str.clone(),
                "evm_context".to_string(),
                "low".to_string(),
            ));
        }
        if expr_str.contains("evm::log(") {
            ops.push(self.build_operation(
                line,
                expr_str.clone(),
                "event".to_string(),
                "medium".to_string(),
            ));
        }

        // Recurse
        match expr {
            Expr::MethodCall(m) => ops.extend(self.analyze_method_call(m, line)),
            Expr::Binary(b) => {
                ops.extend(self.analyze_expr(&b.left, line));
                ops.extend(self.analyze_expr(&b.right, line));
                if matches!(b.op, BinOp::AddAssign(_) | BinOp::SubAssign(_))
                    && self.looks_like_storage_write(&b.left)
                {
                    ops.push(self.build_operation(
                        line,
                        quote!(#expr).to_string(),
                        "storage_write".to_string(),
                        "high".to_string(),
                    ));
                }
            }
            Expr::Field(f) => {
                ops.extend(self.analyze_expr(&f.base, line));
                if self.looks_like_storage_access(&f.base) {
                    ops.push(self.build_operation(
                        line,
                        quote!(#expr).to_string(),
                        "storage_read".to_string(),
                        "high".to_string(),
                    ));
                }
            }
            _ => {}
        }

        ops
    }

    /// Special handling for method calls — mainly interested in `.get()` / `.getter()`
    /// on storage fields (including nested cases).
    fn analyze_method_call(&self, m: &ExprMethodCall, line: usize) -> Vec<Operation> {
        let mut ops = Vec::new();

        ops.extend(self.analyze_expr(&m.receiver, line));

        let method_name = m.method.to_string();
        let receiver_str = quote!(#m.receiver).to_string().replace(" ", "");

        if method_name == "get" || method_name == "getter" {
            let field = extract_storage_variable(&receiver_str);
            let is_nested = receiver_str.contains(".get(");
            let op_name = if is_nested {
                "nested_map_get"
            } else {
                "map::get"
            };
            let code = if is_nested {
                format!("{}.get(...).get(...)", field)
            } else {
                format!("{}.get(...)", field)
            };

            ops.push(Operation {
                line,
                column: 0,
                code,
                operation: op_name.to_string(),
                entity: field.clone(),
                ink: 1200000,
                percentage: 0.0,
                category: "storage_read".to_string(),
                severity: "high".to_string(),
            });
            ops.push(Operation {
                line,
                column: 0,
                code: format!("self.{}", field),
                operation: "storage_field_access".to_string(),
                entity: field.clone(),
                ink: 1200000,
                percentage: 0.0,
                category: "storage_read".to_string(),
                severity: "high".to_string(),
            });
        }

        for arg in &m.args {
            ops.extend(self.analyze_expr(arg, line));
        }

        ops
    }

    /// Heuristic to detect storage read expressions (direct field access or `.get()` calls)
    fn detect_storage_read(&self, expr: &str) -> Option<(String, bool)> {
        let normalized = expr.replace(" ", "");
        if !normalized.contains("self.") {
            return None;
        }

        let get_count = normalized.matches(".get(").count();
        if get_count > 0 {
            let field = extract_storage_variable(expr);
            if !field.is_empty() && field != "unknown" {
                return Some((field, get_count >= 2));
            }
        }

        if normalized.starts_with("self.") && !normalized.contains('(') {
            let field = extract_storage_variable(expr);
            if !field.is_empty() && field != "unknown" {
                return Some((field, false));
            }
        }

        None
    }

    /// Factory method for Operation structs with automatic name & ink estimation
    fn build_operation(
        &self,
        line: usize,
        code: String,
        category: String,
        severity: String,
    ) -> Operation {
        let operation_name = self.detect_operation_name(&code, &category);
        let entity = self.extract_storage_entity(&code);

        let ink = self.estimate_ink_cost(&operation_name, &category);

        Operation {
            line,
            column: 0,
            code,
            operation: operation_name,
            entity,
            ink,
            percentage: 0.0,
            category,
            severity,
        }
    }

    /// Hard-coded ink cost estimates for different categories of operations
    /// (these values reflect observed / documented Stylus behavior as of 2024–2025).
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
                if operation.contains("msg::sender") {
                    300_000
                } else if operation.contains("msg::value") {
                    350_000
                } else if operation.contains("block::") {
                    250_000
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
            _ => false,
        }
    }

    fn looks_like_storage_access(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Path(path) => self.path_starts_with_self(&path.path),
            Expr::Field(field) => self.looks_like_storage_access(&field.base),
            _ => false,
        }
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

    fn path_starts_with_self(&self, path: &syn::Path) -> bool {
        path.segments.first().is_some_and(|seg| seg.ident == "self")
    }

    /// Very simple heuristic for detecting storage mutations
    fn detect_storage_write(&self, expr: &str) -> Option<&'static str> {
        let normalized = expr.replace(" ", "");

        if !normalized.contains("self.") {
            return None;
        }

        if normalized.contains(".set(") || normalized.contains(".insert(") {
            return Some("write()");
        }

        None
    }

    /// Aggregate statistics per category (count, total ink, average, % of function)
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

    /// Detect repeated storage reads of the same field that should probably be cached.
    fn detect_optimizations(&self, operations: &[Operation]) -> Vec<Optimization> {
        let mut optimizations = Vec::new();

        let mut read_map: HashMap<String, Vec<usize>> = HashMap::new();
        for op in operations {
            if op.category == "storage_read" {
                let var = if op.entity != "unknown" {
                    &op.entity
                } else {
                    continue;
                };
                read_map.entry(var.to_string()).or_default().push(op.line);
            }
        }

        for (var, lines) in read_map {
            if lines.len() > 2 && !var.is_empty() {
                let mut unique_lines = lines.clone();
                unique_lines.sort();
                unique_lines.dedup();

                let read_count = unique_lines.len();

                optimizations.push(Optimization {
                    id: format!("cache_{}", var),
                    line: *unique_lines.first().unwrap(),
                    severity: "medium".to_string(),
                    title: format!("Cache repeated storage read: self.{}", var),
                    description: format!(
                        "Field `{}` read {}× → cache to save ~{:.1}M ink",
                        var,
                        read_count,
                        (read_count as f64 - 1.0) * 13.2
                    ),
                    current_code: format!("// Reads at lines: {:?}", unique_lines),
                    suggested_code: format!(
                        "let cached_{} = self.{}.get(...);\n// Use cached_{} instead",
                        var, var, var
                    ),
                    estimated_savings_ink: 13_200_000 * (read_count as u64 - 1),
                    estimated_savings_percentage: 92.0,
                    confidence: "high".to_string(),
                });
            }
        }

        optimizations
    }

    /// Refine operation name string for more readable reporting
    fn detect_operation_name(&self, expr_str: &str, category: &str) -> String {
        let normalized = expr_str.replace(" ", "");

        match category {
            "storage_read" => {
                if normalized.contains(".get(") || normalized.contains(".getter(") {
                    let count = normalized.matches(".get(").count();
                    if count >= 2 {
                        format!("nested_map_get (×{})", count)
                    } else {
                        "map::get".to_string()
                    }
                } else if normalized.starts_with("self.") && !normalized.contains('(') {
                    "storage::load".to_string()
                } else {
                    "storage_read (direct)".to_string()
                }
            }
            "storage_write" => {
                if normalized.contains(".insert(") || normalized.contains(".set(") {
                    if normalized.contains(".get(") {
                        "map::upsert".to_string()
                    } else {
                        "map::insert".to_string()
                    }
                } else {
                    "storage::store".to_string()
                }
            }
            _ => category.to_string(),
        }
    }

    /// Extract storage field name from expression (delegates to free function)
    fn extract_storage_entity(&self, code: &str) -> String {
        extract_storage_variable(code)
    }
}

impl<'a> Visit<'a> for ContractVisitor<'a> {
    /// Visit `impl` blocks and decide whether they look like contract entry points.
    ///
    /// Three detection strategies:
    /// 1. Classic `#[external]` / `#[public]` attributes
    /// 2. Public methods with `&self` / `&mut self` receivers (heuristic)
    /// 3. sol!/macro style indicators (selectors, route/dispatch methods)
    fn visit_item_impl(&mut self, node: &'a ItemImpl) {
        let has_external_attr = node.attrs.iter().any(|attr| {
            attr.path().segments.len() == 1 && {
                let ident = &attr.path().segments[0].ident;
                ident == "external" || ident == "public"
            }
        });

        let has_likely_abi_method = node.items.iter().any(|item| {
            if let ImplItem::Fn(f) = item {
                let name = f.sig.ident.to_string();

                if name == "new"
                    || name == "required_slots"
                    || name.starts_with("__stylus_")
                    || name == "load"
                    || name == "load_mut"
                    || name == "entrypoint"
                    || name == "user_entrypoint"
                    || name == "mark_used"
                    || name == "route"
                    || name == "dispatch"
                {
                    return false;
                }

                let is_public = matches!(f.vis, syn::Visibility::Public(_));

                is_public && f.sig.receiver().is_some() && !name.starts_with('_')
            } else {
                false
            }
        });

        let is_probably_sol_style = self.selector_count >= 2
            || self.has_router_impl
            || node
                .attrs
                .iter()
                .any(|a| a.path().is_ident("stylus_assert_overrides"))
            || node.items.iter().any(|item| {
                if let ImplItem::Fn(f) = item {
                    let n = f.sig.ident.to_string();
                    n == "route" || n == "user_entrypoint" || n == "dispatch"
                } else {
                    false
                }
            });

        let should_analyze = has_external_attr || has_likely_abi_method || is_probably_sol_style;

        if should_analyze {
            for item in &node.items {
                if let ImplItem::Fn(method) = item {
                    let name = method.sig.ident.to_string();

                    if name == "route"
                        || name == "dispatch"
                        || name.starts_with("__")
                        || matches!(
                            name.as_str(),
                            "new"
                                | "load"
                                | "load_mut"
                                | "entrypoint"
                                | "user_entrypoint"
                                | "mark_used"
                                | "required_slots"
                        )
                    {
                        continue;
                    }

                    let signature = quote!(#method.sig).to_string();
                    let start_line = self.function_lines.get(&name).copied().unwrap_or(1);

                    self.analyze_function(name, signature, &method.block.stmts, start_line);
                }
            }
        }

        if node.items.iter().any(|item| {
            if let ImplItem::Fn(f) = item {
                let n = f.sig.ident.to_string();
                n == "route" || n == "user_entrypoint" || n == "dispatch"
            } else {
                false
            }
        }) {
            self.has_router_impl = true;
        }

        syn::visit::visit_item_impl(self, node);
    }

    /// Count occurrences of selector constants to help detect macro-generated dispatchers
    fn visit_item_const(&mut self, node: &'a ItemConst) {
        let name = node.ident.to_string();
        if name.starts_with("SELECTOR_") || name.ends_with("_SELECTOR") {
            self.selector_count += 1;
        }
        syn::visit::visit_item_const(self, node);
    }
}

/// Naive heuristic to extract contract name from the first `pub struct … {` occurrence
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

/// Extract storage field name from expressions like `self.balances.get(key)`
/// using a simple regex anchored after `self.`
fn extract_storage_variable(code: &str) -> String {
    let code_normalized = code.trim().replace(" ", "");
    let re_outer = regex::Regex::new(r"self\.([a-zA-Z_][a-zA-Z0-9_]*)\.").unwrap();

    if let Some(caps) = re_outer.captures(&code_normalized)
        && let Some(m) = caps.get(1)
    {
        let name = m.as_str().to_string();
        if !["mut", "ref", "as", "let", "where", "self", "get", "insert"].contains(&name.as_str()) {
            return name;
        }
    }
    "unknown".to_string()
}
