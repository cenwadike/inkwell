use anyhow::{Context, Result};
use quote::quote;
use syn::{
    Block, Expr, File, ImplItem, ItemImpl, Stmt, parse_file, parse_quote, visit_mut::VisitMut,
};

pub struct Instrumentor {
    /// Counter for generating unique probe IDs
    probe_counter: u32,
    /// Track which operations we're instrumenting
    instrumented_operations: Vec<InstrumentedOperation>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields are used in instrumented output
pub struct InstrumentedOperation {
    pub probe_id: u32,
    pub operation_type: String,
    pub line: usize,
}

impl Instrumentor {
    pub fn new() -> Self {
        Self {
            probe_counter: 0,
            instrumented_operations: Vec::new(),
        }
    }

    /// Instrument contract source code with ink tracking probes
    pub fn instrument(&mut self, source: &str) -> Result<String> {
        // Parse the source code into an AST
        let mut ast = parse_file(source).context("Failed to parse Rust source code")?;

        // Visit and transform the AST
        self.visit_file_mut(&mut ast);

        // Generate the instrumented source code
        let instrumented = self.generate_instrumented_code(&ast)?;

        Ok(instrumented)
    }

    /// Get the list of instrumented operations for reporting
    pub fn get_instrumented_operations(&self) -> &[InstrumentedOperation] {
        &self.instrumented_operations
    }

    /// Generate a unique probe ID
    fn next_probe_id(&mut self) -> u32 {
        let id = self.probe_counter;
        self.probe_counter += 1;
        id
    }

    /// Generate the final instrumented source code with ink tracking runtime
    fn generate_instrumented_code(&self, ast: &File) -> Result<String> {
        let instrumented_ast = quote! {
                #ast

                // Ink tracking runtime - injected by instrumentor
                #[cfg(feature = "ink-profiling")]
                mod __ink_profiling {
                    use std::sync::Mutex;
                    use std::collections::HashMap;

                    static INK_TRACKER: Mutex<Option<InkTracker>> = Mutex::new(None);

                    pub struct InkTracker {
                        probe_data: HashMap<u32, ProbeData>,
                        dry_nib_detections: Vec<DryNibBug>,
                        start_ink: u64,
                    }

                    #[derive(Clone)]
                    pub struct ProbeData {
                        pub probe_id: u32,
                        pub ink_before: u64,
                        pub ink_after: u64,
                        pub count: u64,
                        pub return_data_size: Option<usize>,
                        pub operation_type: Option<String>,
                    }

                    #[derive(Clone, Debug)]
                    pub struct DryNibBug {
                        pub probe_id: u32,
                        pub operation: String,
                        pub ink_charged: u64,
                        pub actual_return_size: usize,
                        pub expected_overhead: u64,
                        pub overcharge_amount: u64,
                    }

                    impl InkTracker {
                        pub fn init() {
                            let mut tracker = INK_TRACKER.lock().unwrap();
                            *tracker = Some(InkTracker {
                                probe_data: HashMap::new(),
                                dry_nib_detections: Vec::new(),
                                start_ink: Self::read_ink_counter(),
                            });
                        }

                        pub fn record_before(probe_id: u32) -> u64 {
                            Self::read_ink_counter()
                        }

                        pub fn record_after(probe_id: u32, ink_before: u64, operation_type: Option<&str>) {
                            let ink_after = Self::read_ink_counter();
                            let ink_consumed = ink_before.saturating_sub(ink_after);

                            let mut tracker = INK_TRACKER.lock().unwrap();

                            if let Some(ref mut t) = *tracker {
                                t.probe_data
                                    .entry(probe_id)
                                    .and_modify(|data| {
                                        data.ink_after = ink_after;
                                        data.count += 1;
                                    })
                                    .or_insert(ProbeData {
                                        probe_id,
                                        ink_before,
                                        ink_after,
                                        count: 1,
                                        return_data_size: None,
                                        operation_type: operation_type.map(|s| s.to_string()),
                                    });

                                // Detect dry nib bugs for host calls
                                if let Some(op_type) = operation_type {
                                    if op_type.contains("storage_read") || op_type.contains("storage_write")
                                       || op_type.contains("msg_") || op_type.contains("block_") {
                                        Self::check_dry_nib(
                                            &mut t.dry_nib_detections,
                                            probe_id,
                                            op_type,
                                            ink_consumed,
                                        );
                                    }
                                }
                            }
                        }

                        pub fn record_after_with_size(probe_id: u32, ink_before: u64, return_size: usize, operation_type: Option<&str>) {
                            let ink_after = Self::read_ink_counter();
                            let ink_consumed = ink_before.saturating_sub(ink_after);

                            let mut tracker = INK_TRACKER.lock().unwrap();

                            if let Some(ref mut t) = *tracker {
                                t.probe_data
                                    .entry(probe_id)
                                    .and_modify(|data| {
                                        data.ink_after = ink_after;
                                        data.count += 1;
                                        data.return_data_size = Some(return_size);
                                    })
                                    .or_insert(ProbeData {
                                        probe_id,
                                        ink_before,
                                        ink_after,
                                        count: 1,
                                        return_data_size: Some(return_size),
                                        operation_type: operation_type.map(|s| s.to_string()),
                                    });

                                // Detect dry nib bugs
                                if let Some(op_type) = operation_type {
                                    Self::check_dry_nib_with_size(
                                        &mut t.dry_nib_detections,
                                        probe_id,
                                        op_type,
                                        ink_consumed,
                                        return_size,
                                    );
                                }
                            }
                        }

                                                fn check_dry_nib(
                            detections: &mut Vec<DryNibBug>,
                            probe_id: u32,
                            operation: &str,
                            ink_charged: u64,
                        ) {
                            let expected_base = match () {
                                _ if operation.contains("storage_read")  => 650_000,
                                _ if operation.contains("storage_write") => 900_000,
                                _ if operation.contains("msg_sender")    =>  80_000,
                                _ => 50_000,
                            };

                            // Much more aggressive for dry-nib detection
                            let tolerance = 200_000; // flag anything over ~200k extra
                            if ink_charged > expected_base + tolerance {
                                detections.push(DryNibBug {
                                    probe_id,
                                    operation: operation.to_string(),
                                    ink_charged,
                                    actual_return_size: 0,
                                    expected_overhead: expected_base,
                                    overcharge_amount: ink_charged - expected_base,
                                });
                            }
                        }

                        fn check_dry_nib_with_size(
                            detections: &mut Vec<DryNibBug>,
                            probe_id: u32,
                            operation: &str,
                            ink_charged: u64,
                            actual_size: usize,
                        ) {
                            let base_cost = if operation.contains("storage_read") { 650_000 } else { 900_000 };
                            let fair_variable = (actual_size as u64) * 30; // realistic copying cost

                            let expected_overhead = base_cost + fair_variable;
                            let tolerance = 250_000.max(expected_overhead / 4);

                            if ink_charged > expected_overhead + tolerance {
                                detections.push(DryNibBug {
                                    probe_id,
                                    operation: operation.to_string(),
                                    ink_charged,
                                    actual_return_size: actual_size,
                                    expected_overhead,
                                    overcharge_amount: ink_charged - expected_overhead,
                                });
                            }
                        }

                        // Optional: Improve dump_report formatting for clarity
                        pub fn dump_report() -> String {
                            let tracker = INK_TRACKER.lock().unwrap();
                            if let Some(ref t) = *tracker {
                                let mut report = String::new();

                                report.push_str(&format!(
                                    "Total ink used: ~{} (start → current)\n\n",
                                    t.start_ink.saturating_sub(Self::read_ink_counter())
                                ));

                                report.push_str("Probe measurements:\n");
                                for (id, data) in &t.probe_data {
                                    report.push_str(&format!(
                                        "Probe #{} ({}): {} ink consumed (before={}, after={})\n",
                                        id, data.operation_type.as_deref().unwrap_or("?"),
                                        data.ink_before.saturating_sub(data.ink_after),
                                        data.ink_before, data.ink_after
                                    ));
                                }

                                if !t.dry_nib_detections.is_empty() {
                                    report.push_str("\n=== DRY NIB OVERCHARGE BUGS DETECTED ===\n");
                                    report.push_str("These are cases where real ink used >> expected fair cost\n");
                                    report.push_str("(likely buffer padding / allocation waste on small returns)\n\n");

                                    for bug in &t.dry_nib_detections {
                                        let pct = if bug.expected_overhead > 0 {
                                            (bug.overcharge_amount as f64 / bug.expected_overhead as f64) * 100.0
                                        } else { 0.0 };

                                        report.push_str(&format!(
                                            "🐛 Probe {}: {}\n   Charged:   {} ink\n   Expected:  {} ink\n   Overcharge: {} ink ({:.1}%)\n   Return size: {} bytes\n\n",
                                            bug.probe_id, bug.operation, bug.ink_charged, bug.expected_overhead,
                                            bug.overcharge_amount, pct, bug.actual_return_size
                                        ));
                                    }
                                }

                                report
                            } else {
                                "{}".to_string()
                            }
                        }

                        // Platform-specific ink counter reading
                        #[cfg(target_arch = "wasm32")]
                        fn read_ink_counter() -> u64 {
                            // Read from Stylus VM ink register
                            unsafe {
                                stylus_sdk::hostio::ink_left()
                            }
                        }

                        #[cfg(not(target_arch = "wasm32"))]
                        fn read_ink_counter() -> u64 {
                            // Fallback for testing - use monotonic time as proxy
                            use std::time::{SystemTime, UNIX_EPOCH};
                            SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_nanos() as u64
                        }
                    }

                #[inline(always)]
                pub fn probe_before(id: u32) -> u64 {
                    InkTracker::record_before(id)
                }

                #[inline(always)]
                pub fn probe_after(id: u32, before: u64, operation_type: Option<&str>) {
                    InkTracker::record_after(id, before, operation_type)
                }

                #[inline(always)]
                pub fn probe_after_with_size(id: u32, before: u64, size: usize, operation_type: Option<&str>) {
                    InkTracker::record_after_with_size(id, before, size, operation_type)
                }
            }
        };

        Ok(instrumented_ast.to_string())
    }

    /// Inject probe before and after a statement
    fn inject_probe(&mut self, stmt: &Stmt, operation_type: &str) -> Vec<Stmt> {
        let probe_id = self.next_probe_id();

        // Record this instrumentation
        self.instrumented_operations.push(InstrumentedOperation {
            probe_id,
            operation_type: operation_type.to_string(),
            line: 0, // Line numbers will be preserved from original AST
        });

        let stmt_clone = stmt.clone();
        let op_type_lit = operation_type;

        // Check if this is a storage read that we can measure size
        if operation_type.contains("storage_read") {
            vec![
                parse_quote! {
                    #[cfg(feature = "ink-profiling")]
                    let __ink_before = __ink_profiling::probe_before(#probe_id);
                },
                // Wrap the original statement to capture result
                parse_quote! {
                    #[cfg(feature = "ink-profiling")]
                    let __result = { #stmt_clone };
                },
                parse_quote! {
                    #[cfg(not(feature = "ink-profiling"))]
                    #stmt_clone
                },
                parse_quote! {
                    #[cfg(feature = "ink-profiling")]
                    {
                        let __size = std::mem::size_of_val(&__result);
                        __ink_profiling::probe_after_with_size(#probe_id, __ink_before, __size, Some(#op_type_lit));
                        __result
                    }
                },
            ]
        } else {
            vec![
                parse_quote! {
                    #[cfg(feature = "ink-profiling")]
                    let __ink_before = __ink_profiling::probe_before(#probe_id);
                },
                stmt_clone,
                parse_quote! {
                    #[cfg(feature = "ink-profiling")]
                    __ink_profiling::probe_after(#probe_id, __ink_before, Some(#op_type_lit));
                },
            ]
        }
    }

    /// Check if an expression is a storage operation
    fn is_storage_operation(expr: &Expr) -> bool {
        let expr_str = quote!(#expr).to_string();
        let normalized = expr_str
            .replace(" . ", ".")
            .replace(". ", ".")
            .replace(" (", "(");

        normalized.contains("self.")
            && (normalized.contains(".get(")
                || normalized.contains(".insert(")
                || normalized.contains(".set(")
                || normalized.contains("self.") && !normalized.contains('('))
    }

    /// Check if an expression is an expensive EVM operation
    fn is_expensive_operation(expr: &Expr) -> bool {
        let expr_str = quote!(#expr).to_string();
        expr_str.contains("msg::sender()")
            || expr_str.contains("msg::value()")
            || expr_str.contains("evm::log(")
            || expr_str.contains("block::")
            || expr_str.contains(".call(")
            || expr_str.contains("Call::new")
            || expr_str.contains("keccak256")
            || expr_str.contains("sha256")
    }

    /// Classify the operation type for an expression
    fn classify_operation(expr: &Expr) -> Option<&'static str> {
        let expr_str = quote!(#expr).to_string();
        let normalized = expr_str
            .replace(" . ", ".")
            .replace(". ", ".")
            .replace(" (", "(");

        if normalized.contains(".get(") || normalized.contains(".at(") {
            Some("storage_read")
        } else if normalized.contains(".insert(") || normalized.contains(".set(") {
            Some("storage_write")
        } else if normalized.contains("evm::log(") {
            Some("event_emit")
        } else if normalized.contains("msg::sender()") {
            Some("msg_sender")
        } else if normalized.contains("msg::value()") {
            Some("msg_value")
        } else if normalized.contains("block::") {
            Some("block_info")
        } else {
            None
        }
    }
}

impl VisitMut for Instrumentor {
    /// Visit implementation blocks and instrument #[external] functions
    fn visit_item_impl_mut(&mut self, node: &mut ItemImpl) {
        // Check if this is an #[external] impl block
        let is_external = node
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("external"));

        if is_external {
            for item in &mut node.items {
                if let ImplItem::Fn(method) = item {
                    // Instrument the function body
                    self.visit_block_mut(&mut method.block);
                }
            }
        }

        // Continue visiting nested items
        syn::visit_mut::visit_item_impl_mut(self, node);
    }

    /// Visit blocks and instrument expensive operations
    fn visit_block_mut(&mut self, node: &mut Block) {
        let mut new_stmts = Vec::new();

        for stmt in &node.stmts {
            // Check if this statement contains operations we want to instrument
            let should_instrument = match stmt {
                Stmt::Local(local) => {
                    if let Some(init) = &local.init {
                        Self::is_storage_operation(&init.expr)
                            || Self::is_expensive_operation(&init.expr)
                    } else {
                        false
                    }
                }
                Stmt::Expr(expr, _) => {
                    Self::is_storage_operation(expr) || Self::is_expensive_operation(expr)
                }
                _ => false,
            };

            if should_instrument {
                // Determine operation type
                let operation_type = match stmt {
                    Stmt::Local(local) => {
                        if let Some(init) = &local.init {
                            Self::classify_operation(&init.expr)
                        } else {
                            None
                        }
                    }
                    Stmt::Expr(expr, _) => Self::classify_operation(expr),
                    _ => None,
                };

                if let Some(op_type) = operation_type {
                    // Inject probes around this statement
                    new_stmts.extend(self.inject_probe(stmt, op_type));
                } else {
                    new_stmts.push(stmt.clone());
                }
            } else {
                new_stmts.push(stmt.clone());
            }
        }

        node.stmts = new_stmts;

        // Continue visiting nested blocks
        syn::visit_mut::visit_block_mut(self, node);
    }
}

impl Default for Instrumentor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instrumented_operations_tracking() {
        let mut instrumentor = Instrumentor::new();

        let source = r#"
            #[external]
            impl Contract {
                pub fn transfer(&mut self) {
                    let balance = self.balances.get(msg::sender());
                    self.balances.insert(msg::sender(), balance - 100);
                }
            }
        "#;

        let _ = instrumentor.instrument(source);
        let ops = instrumentor.get_instrumented_operations();

        assert!(ops.len() >= 2); // At least storage read and write
        assert!(ops.iter().any(|op| op.operation_type == "storage_read"));
        assert!(ops.iter().any(|op| op.operation_type == "storage_write"));
    }

    #[test]
    fn test_non_external_not_instrumented() {
        let mut instrumentor = Instrumentor::new();

        let source = r#"
            impl Helper {
                fn internal_fn(&self) {
                    let value = some_value;
                }
            }
        "#;

        let result = instrumentor.instrument(source);
        assert!(result.is_ok());

        // Should not instrument internal functions
        let ops = instrumentor.get_instrumented_operations();
        assert_eq!(ops.len(), 0);
    }

    #[test]
    fn test_probe_id_generation() {
        let mut instrumentor = Instrumentor::new();

        assert_eq!(instrumentor.next_probe_id(), 0);
        assert_eq!(instrumentor.next_probe_id(), 1);
        assert_eq!(instrumentor.next_probe_id(), 2);
    }

    #[test]
    fn test_is_storage_operation_detection() {
        let expr: Expr = syn::parse_str("self.balances.get(addr)").unwrap();
        assert!(Instrumentor::is_storage_operation(&expr));

        let expr: Expr = syn::parse_str("self.count.insert(5)").unwrap();
        assert!(Instrumentor::is_storage_operation(&expr));

        let expr: Expr = syn::parse_str("local_var + 5").unwrap();
        assert!(!Instrumentor::is_storage_operation(&expr));
    }

    #[test]
    fn test_instrument_multiple_functions() {
        let mut instrumentor = Instrumentor::new();

        let source = r#"
            #[external]
            impl Token {
                pub fn transfer(&mut self, to: Address, amount: U256) {
                    let balance = self.balances.get(msg::sender());
                    self.balances.insert(to, amount);
                }

                pub fn approve(&mut self, spender: Address, amount: U256) {
                    self.allowances.insert(msg::sender(), spender);
                }
            }
        "#;

        let result = instrumentor.instrument(source);
        assert!(result.is_ok());

        let ops = instrumentor.get_instrumented_operations();
        assert!(ops.len() >= 3); // Multiple storage operations across functions
    }

    #[test]
    fn test_generated_runtime_includes_all_components() {
        let instrumentor = Instrumentor::new();
        let ast: File = syn::parse_str("").unwrap();

        let code = instrumentor.generate_instrumented_code(&ast).unwrap();

        // Verify runtime components are present
        assert!(code.contains("mod __ink_profiling"));
        assert!(code.contains("INK_TRACKER"));
        assert!(code.contains("InkTracker"));
        assert!(code.contains("ProbeData"));
        assert!(code.contains("probe_before"));
        assert!(code.contains("probe_after"));
        assert!(code.contains("read_ink_counter"));
    }

    #[test]
    fn test_default_implementation() {
        let instrumentor = Instrumentor::default();
        assert_eq!(instrumentor.probe_counter, 0);
        assert_eq!(instrumentor.instrumented_operations.len(), 0);
    }

    #[test]
    fn test_inject_probe_generates_correct_statements() {
        let mut instrumentor = Instrumentor::new();
        let stmt: Stmt = syn::parse_str("let x = 5;").unwrap();

        let probes = instrumentor.inject_probe(&stmt, "test_op");

        assert_eq!(probes.len(), 3); // before, stmt, after
        assert_eq!(instrumentor.get_instrumented_operations().len(), 1);
        assert_eq!(
            instrumentor.get_instrumented_operations()[0].operation_type,
            "test_op"
        );
    }

    #[test]
    fn test_normalization_in_classification() {
        // Test that spacing variations are handled correctly
        let expr1: Expr = syn::parse_str("self . balances . get ( key )").unwrap();
        let expr2: Expr = syn::parse_str("self.balances.get(key)").unwrap();

        assert_eq!(
            Instrumentor::classify_operation(&expr1),
            Instrumentor::classify_operation(&expr2)
        );
    }

    #[test]
    fn test_instrument_storage_operations() {
        let mut instrumentor = Instrumentor::new();

        let source = r#"
            #[external]
            impl Counter {
                pub fn increment(&mut self) {
                    let current = self.count.get();
                    self.count.insert(current + 1);
                }
            }
        "#;

        let result = instrumentor.instrument(source);
        assert!(result.is_ok());

        let instrumented = result.unwrap();
        assert!(instrumented.contains("probe_before"));
        assert!(instrumented.contains("probe_after"));
        assert!(instrumented.contains("InkTracker"));
        assert!(instrumented.contains("DryNibBug"));
    }

    #[test]
    fn test_dry_nib_detection_structures() {
        let mut instrumentor = Instrumentor::new();
        let source = r#"
            #[external]
            impl Contract {
                pub fn read(&self) -> U256 {
                    self.value.get()
                }
            }
        "#;

        let result = instrumentor.instrument(source).unwrap();
        assert!(result.contains("dry_nib_detections"));
        assert!(result.contains("check_dry_nib"));
    }
}
