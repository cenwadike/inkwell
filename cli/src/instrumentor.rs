use anyhow::{Context, Result};
use quote::quote;
use syn::{
    Block, Expr, File, ImplItem, ItemImpl, Stmt, parse_file, parse_quote, visit_mut::VisitMut,
};

/// Instrumentation engine for Stylus / Arbitrum Rust contracts.
///
/// This type traverses the AST of a contract, identifies storage operations and
/// other potentially expensive calls (storage reads/writes, msg::sender, events, etc.),
/// and injects runtime probes to measure actual ink consumption at runtime when
/// called with the `ink-profiling` feature.
///
/// After instrumentation, the code includes:
/// - Probe points before/after expensive operations
/// - A thread-safe `InkTracker` singleton that records measurements
/// - Dry-nib overcharge detection (buffer allocation waste)
/// - A human-readable report generator
pub struct Instrumentor {
    probe_counter: u32,
    instrumented_operations: Vec<InstrumentedOperation>,
}

/// Metadata record for each inserted probe point (used for offline analysis
/// or test verification of what was instrumented).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct InstrumentedOperation {
    /// Unique numeric identifier of this probe
    pub probe_id: u32,
    /// Classified operation type (storage_read, storage_write, msg_sender, etc.)
    pub operation_type: String,
    /// Approximate line number where the probe was inserted (currently always 0)
    pub line: usize,
}

impl Instrumentor {
    /// Creates a new, empty instrumentor.
    pub fn new() -> Self {
        Self {
            probe_counter: 0,
            instrumented_operations: Vec::new(),
        }
    }

    /// Parses the source code, instruments eligible operations in public/external
    /// functions, and returns the full instrumented source as a `String`.
    ///
    /// # Errors
    /// Returns `Err` if the input cannot be parsed as valid Rust syntax.
    pub fn instrument(&mut self, source: &str) -> Result<String> {
        let mut ast = parse_file(source).context("Failed to parse Rust source code")?;
        self.visit_file_mut(&mut ast);
        let instrumented = self.generate_instrumented_code(&ast)?;
        Ok(instrumented)
    }

    /// Returns a slice of all probe points that were inserted during instrumentation.
    ///
    /// Useful for tests or offline analysis of which operations were considered expensive.
    pub fn get_instrumented_operations(&self) -> &[InstrumentedOperation] {
        &self.instrumented_operations
    }

    /// Generates the next sequential probe identifier and increments the counter.
    fn next_probe_id(&mut self) -> u32 {
        let id = self.probe_counter;
        self.probe_counter += 1;
        id
    }

    /// Produces the final instrumented source code by appending a profiling module
    /// (`__ink_profiling`) that contains:
    /// - Global `INK_TRACKER` singleton
    /// - `InkTracker` struct + methods for recording measurements
    /// - Dry-nib detection logic
    /// - Helper probe functions (`probe_before`, `probe_after`, `probe_after_with_size`)
    fn generate_instrumented_code(&self, ast: &File) -> Result<String> {
        let instrumented_ast = quote! {
            #ast

            #[cfg(feature = "ink-profiling")]
            mod __ink_profiling {
                use std::sync::Mutex;
                use std::collections::HashMap;

                static INK_TRACKER: Mutex<Option<InkTracker>> = Mutex::new(None);

                /// Global runtime tracker for ink measurements and dry-nib detections.
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
                    /// Initializes the global tracker (called once per contract execution).
                    pub fn init() {
                        let mut tracker = INK_TRACKER.lock().unwrap();
                        *tracker = Some(InkTracker {
                            probe_data: HashMap::new(),
                            dry_nib_detections: Vec::new(),
                            start_ink: Self::read_ink_counter(),
                        });
                    }

                    /// Records ink counter value before an operation.
                    pub fn record_before(probe_id: u32) -> u64 {
                        Self::read_ink_counter()
                    }

                    /// Records ink after a normal (non-sized-return) operation and
                    /// performs dry-nib checks when appropriate.
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

                    /// Variant for operations that return data whose size affects cost
                    /// (mainly storage reads).
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

                    /// Basic dry-nib check when return size is not available.
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

                        let tolerance = 200_000;
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

                    /// Dry-nib check that accounts for actual returned data size.
                    fn check_dry_nib_with_size(
                        detections: &mut Vec<DryNibBug>,
                        probe_id: u32,
                        operation: &str,
                        ink_charged: u64,
                        actual_size: usize,
                    ) {
                        let base_cost = if operation.contains("storage_read") { 650_000 } else { 900_000 };
                        let fair_variable = (actual_size as u64) * 30;

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

                    /// Generates a human-readable report of all measurements and detected issues.
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

                    #[cfg(target_arch = "wasm32")]
                    fn read_ink_counter() -> u64 {
                        unsafe {
                            stylus_sdk::hostio::ink_left()
                        }
                    }

                    #[cfg(not(target_arch = "wasm32"))]
                    fn read_ink_counter() -> u64 {
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

    /// Wraps a statement with probe calls when compiled with `ink-profiling`.
    ///
    /// For storage reads, also captures the size of the returned value.
    fn inject_probe(&mut self, stmt: &Stmt, operation_type: &str) -> Vec<Stmt> {
        let probe_id = self.next_probe_id();

        self.instrumented_operations.push(InstrumentedOperation {
            probe_id,
            operation_type: operation_type.to_string(),
            line: 0,
        });

        let stmt_clone = stmt.clone();
        let op_type_lit = operation_type;

        if operation_type.contains("storage_read") {
            vec![
                parse_quote! {
                    #[cfg(feature = "ink-profiling")]
                    let __ink_before = __ink_profiling::probe_before(#probe_id);
                },
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

    /// Heuristic: does this expression look like a storage access?
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
                || (normalized.contains("self.") && !normalized.contains('(')))
    }

    /// Heuristic: is this expression likely to be expensive (host call, crypto, etc.)?
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

    /// Classifies an expression into a probe category string (used both for
    /// instrumentation and dry-nib classification).
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
    /// Visits `impl` blocks and decides which methods to instrument.
    ///
    /// Instrumentation occurs when:
    /// - The impl has `#[external]` or `#[public]`
    /// - The method is `pub`
    /// - The method itself has `#[external]` or `#[public]`
    fn visit_item_impl_mut(&mut self, node: &mut ItemImpl) {
        let has_api_attribute = node
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("external") || attr.path().is_ident("public"));

        if has_api_attribute {
            for item in &mut node.items {
                if let ImplItem::Fn(method) = item {
                    let is_public_fn = matches!(method.vis, syn::Visibility::Public(_));

                    let has_fn_attribute = method.attrs.iter().any(|attr| {
                        attr.path().is_ident("external") || attr.path().is_ident("public")
                    });

                    let should_instrument = is_public_fn
                        || has_fn_attribute
                        || node.attrs.iter().any(|a| a.path().is_ident("external"));

                    if should_instrument {
                        self.visit_block_mut(&mut method.block);
                    }
                }
            }
        } else {
            for item in &mut node.items {
                if let ImplItem::Fn(method) = item {
                    let has_fn_attribute = method.attrs.iter().any(|attr| {
                        attr.path().is_ident("external") || attr.path().is_ident("public")
                    });

                    if has_fn_attribute {
                        self.visit_block_mut(&mut method.block);
                    }
                }
            }
        }

        syn::visit_mut::visit_item_impl_mut(self, node);
    }

    /// Rewrites block statements, inserting probes around detected expensive operations.
    fn visit_block_mut(&mut self, node: &mut Block) {
        let mut new_stmts = Vec::new();

        for stmt in &node.stmts {
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
                    new_stmts.extend(self.inject_probe(stmt, op_type));
                } else {
                    new_stmts.push(stmt.clone());
                }
            } else {
                new_stmts.push(stmt.clone());
            }
        }

        node.stmts = new_stmts;
        syn::visit_mut::visit_block_mut(self, node);
    }
}

impl Default for Instrumentor {
    fn default() -> Self {
        Self::new()
    }
}
