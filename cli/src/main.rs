use alloy::network::EthereumWallet;
use alloy::primitives::{Bytes, U256, keccak256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use alloy::signers::Signer;
use alloy::signers::local::PrivateKeySigner;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;
use toml::Value;

mod analyzer;
mod instrumentor;
mod reporter;
mod types;

use analyzer::analyze_contract;
use instrumentor::Instrumentor;
use types::{Decorations, VsCodeDecorations};

/// Command-line interface for Inkwell — a Stylus contract ink/gas analysis & profiling tool.
///
/// Subcommands:
///   dip        → static analysis of ink consumption patterns
///   instrument → insert runtime ink measurement probes
#[derive(Parser)]
#[command(name = "inkwell")]
#[command(about = "🧪 Inkwell - Dive deep into Stylus contract gas analysis")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Available subcommands.
#[derive(Subcommand)]
enum Commands {
    /// 🔬 Dip into your contract - analyze ink consumption patterns, detect dry-nib bugs,
    /// suggest caching optimizations, and generate VS Code decorations.
    #[command(alias = "d")]
    Dip {
        /// Path to the Rust contract file (usually `src/lib.rs`)
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Target specific function for analysis (if omitted, analyzes all public/external)
        #[arg(short, long)]
        function: Option<String>,

        /// Output format: compact (default), detailed, json
        #[arg(short, long, default_value = "compact")]
        output: String,

        /// Ink threshold for highlighting operations in compact view (default: 100_000)
        #[arg(long, default_value = "100000")]
        threshold: u64,

        /// Disable colored terminal output
        #[arg(long)]
        no_color: bool,

        /// Enable real on-chain ink profiling (requires --rpc-url and --private-key)
        #[arg(short, long)]
        profile: bool,

        /// RPC endpoint for profiling mode (default: local Stylus dev node)
        #[arg(long, default_value = "http://localhost:8547")]
        rpc_url: String,

        /// Private key (hex) for deploying and calling the contract in profiling mode
        #[arg(long)]
        private_key: Option<String>,

        /// Chain ID to use for transactions (default: 1337 for local dev)
        #[arg(long, default_value = "1337")]
        chain_id: u64,

        /// 0x-prefixed hex calldata for the profiling transaction
        #[arg(long)]
        calldata: Option<String>,

        /// Value (in wei) to send with the profiling transaction
        #[arg(long, default_value = "0")]
        value: String,

        /// Where to save the instrumented contract source (profiling mode only)
        #[arg(long, default_value = "instrumented_contract.rs")]
        instrumented_output: PathBuf,
    },

    /// 🧬 Instrument your contract with runtime ink tracking probes.
    ///
    /// Adds measurement points around storage/host calls; when compiled with
    /// `--features ink-profiling`, produces runtime ink usage reports and dry-nib warnings.
    #[command(alias = "i")]
    Instrument {
        /// Path to the Rust contract file to instrument
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Output path for the instrumented contract source
        #[arg(short, long, default_value = "instrumented_contract.rs")]
        output: PathBuf,

        /// Disable colored terminal output
        #[arg(long)]
        no_color: bool,
    },
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Dip {
            file,
            function,
            output,
            threshold,
            no_color,
            profile,
            rpc_url,
            private_key,
            chain_id,
            calldata,
            value,
            instrumented_output,
        } => {
            if !file.exists() {
                anyhow::bail!("Source file not found: {}", file.display());
            }

            let source = fs::read_to_string(&file)?;

            if profile {
                run_profiling_mode(
                    &file,
                    &source,
                    &rpc_url,
                    private_key,
                    chain_id,
                    calldata,
                    &value,
                    &instrumented_output,
                    no_color,
                )
                .await?;
            } else {
                run_analysis_mode(
                    &file,
                    &source,
                    function.as_deref(),
                    &output,
                    threshold,
                    no_color,
                )?;
            }
        }
        Commands::Instrument {
            file,
            output,
            no_color,
        } => {
            if !file.exists() {
                anyhow::bail!("Source file not found: {}", file.display());
            }

            let source = fs::read_to_string(&file)?;
            run_instrumentation_mode(&source, &output, no_color)?;
        }
    }

    Ok(())
}

/// Runs the instrumentation-only mode: adds probes and saves the modified source.
fn run_instrumentation_mode(source: &str, output_path: &Path, no_color: bool) -> Result<()> {
    let mut instrumentor = Instrumentor::new();
    let instrumented = instrumentor.instrument(source)?;

    fs::write(output_path, &instrumented)?;

    let ops = instrumentor.get_instrumented_operations();
    print_instrumentation_summary(no_color, ops.len(), ops);

    if !no_color {
        println!("\n{}", "📝 Next steps:".bright_yellow().bold());
        println!(
            "   {}",
            "cargo build --release --target wasm32-unknown-unknown --features ink-profiling"
                .dimmed()
        );
        println!(
            "\n{} Instrumented contract saved to: {}",
            "✓".bright_green(),
            output_path.display().to_string().bright_white()
        );
    } else {
        println!("\nNext steps:");
        println!(
            "   cargo build --release --target wasm32-unknown-unknown --features ink-profiling"
        );
        println!(
            "\nInstrumented contract saved to: {}",
            output_path.display()
        );
    }

    Ok(())
}

/// Prints a summary of how many probes were injected and their breakdown by type.
fn print_instrumentation_summary(
    no_color: bool,
    total: usize,
    ops: &[instrumentor::InstrumentedOperation],
) {
    if no_color {
        println!("\n═══════════════════════════════════════════");
        println!("  Instrumentation Complete");
        println!("═══════════════════════════════════════════");
        println!("\nTotal probes injected: {}", total);

        let mut counts: HashMap<&str, usize> = HashMap::new();
        for op in ops {
            *counts.entry(&op.operation_type).or_insert(0) += 1;
        }

        println!("\nBreakdown by operation type:");
        for (typ, cnt) in counts {
            println!("  • {}: {}", typ, cnt);
        }
    } else {
        println!("\n{}", "═".repeat(45).dimmed());
        println!("  {}", "🧬 Instrumentation Complete".bright_green().bold());
        println!("{}", "═".repeat(45).dimmed());
        println!(
            "\n{} Probes injected: {}",
            "💉".bright_cyan(),
            total.to_string().bright_yellow().bold()
        );

        let mut counts: HashMap<&str, usize> = HashMap::new();
        for op in ops {
            *counts.entry(&op.operation_type).or_insert(0) += 1;
        }

        println!("\n{}", "Breakdown by operation type:".dimmed());
        for (typ, cnt) in counts {
            println!(
                "  {} {}: {}",
                "•".bright_cyan(),
                typ,
                cnt.to_string().bright_white()
            );
        }
    }
}

/// Runs static analysis mode: parses, analyzes ink usage, prints report,
/// saves JSON output, and generates VS Code decoration data.
fn run_analysis_mode(
    source_path: &Path,
    source_content: &str,
    function: Option<&str>,
    output_format: &str,
    threshold: u64,
    no_color: bool,
) -> Result<()> {
    let absolute_source = fs::canonicalize(source_path).with_context(|| {
        format!(
            "Failed to canonicalize source path: {}",
            source_path.display()
        )
    })?;

    let source_dir = absolute_source
        .parent()
        .context("Source file has no parent directory")?
        .to_path_buf();

    eprintln!(
        "{} Analyzed file (absolute): {}",
        "🔍".bright_blue(),
        absolute_source.display()
    );
    eprintln!(
        "{} Resolved source directory: {}",
        "📂".bright_blue(),
        source_dir.display()
    );

    let project_root = find_project_root(&source_dir)
        .or_else(|| {
            eprintln!(
                "{} No Cargo.toml found → falling back to source directory",
                "⚠️".bright_yellow()
            );
            Some(source_dir.clone())
        })
        .unwrap_or_else(|| {
            eprintln!(
                "{} Using current dir as last resort fallback",
                "⚠️".bright_yellow()
            );
            std::env::current_dir().unwrap_or_default()
        });

    eprintln!(
        "{} Using project root for analysis/expansion: {}",
        "📍".bright_cyan(),
        project_root.display()
    );

    let relative_path = absolute_source
        .strip_prefix(&project_root)
        .unwrap_or(&absolute_source)
        .to_path_buf();

    let source_to_analyze = get_analyzable_source(source_content, &absolute_source)?;

    let analysis = match analyze_contract(&source_to_analyze, function, relative_path.clone()) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("\n{}", "ERROR during analysis:".bright_red().bold());
            eprintln!("{:#}", e);
            eprintln!("\nIf using sol! macros, ensure 'cargo +nightly expand' works.");
            std::process::exit(1);
        }
    };

    let inkwell_dir = project_root.join(".inkwell");
    fs::create_dir_all(&inkwell_dir)?;

    let reporter = reporter::Reporter::new(output_format, threshold, !no_color);
    reporter.print_report(&analysis)?;

    let decorations = match reporter.generate_vscode_decorations(&analysis) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Warning: Could not generate decorations: {}", e);
            VsCodeDecorations {
                file: analysis.file.clone(),
                function: "analysis_partial".to_string(),
                total_ink: 0,
                gas_equivalent: 0,
                decorations: Decorations::default(),
            }
        }
    };

    fs::write(
        project_root.join("ink-report.json"),
        serde_json::to_string_pretty(&analysis)?,
    )?;

    fs::write(
        inkwell_dir.join("decorations.json"),
        serde_json::to_string_pretty(&decorations)?,
    )?;

    if !no_color {
        println!(
            "\n{}",
            "Analysis complete → decorations saved".bright_green()
        );
    }

    Ok(())
}

/// Helper to locate the nearest `Cargo.toml` upward from a starting directory.
fn find_project_root(start_dir: &Path) -> Option<PathBuf> {
    let mut current = start_dir.to_path_buf();
    loop {
        if current.join("Cargo.toml").exists() {
            return Some(current);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

/// Runs on-chain profiling mode:
/// 1. Instruments the contract
/// 2. Builds WASM with ink-profiling feature
/// 3. Deploys to the chain
/// 4. Executes provided calldata (if any)
/// 5. Calls `get_ink_report()` and prints the runtime report
#[allow(clippy::too_many_arguments)]
async fn run_profiling_mode(
    original_file: &Path,
    source: &str,
    rpc_url: &str,
    private_key: Option<String>,
    chain_id: u64,
    calldata: Option<String>,
    value: &str,
    instrumented_output: &Path,
    no_color: bool,
) -> Result<()> {
    if !no_color {
        println!("\n{}", "═".repeat(60).bright_red());
        println!("  {}", "🔥 REAL INK PROFILING MODE".bright_red().bold());
        println!("{}", "═".repeat(60).bright_red());
        println!(
            "\n{} RPC endpoint: {}",
            "🌐".bright_cyan(),
            rpc_url.bright_white()
        );
    } else {
        println!("\n═══════════════════════════════════════════");
        println!("  REAL INK PROFILING MODE");
        println!("═══════════════════════════════════════════");
        println!("\nRPC endpoint: {}", rpc_url);
    }

    let privkey_hex = private_key
        .as_ref()
        .context("--private-key is required for profiling mode")?;

    let pk_bytes = hex::decode(privkey_hex.trim_start_matches("0x"))?;
    let signer = PrivateKeySigner::from_slice(&pk_bytes)?.with_chain_id(Some(chain_id));
    let wallet = EthereumWallet::from(signer);

    let provider = ProviderBuilder::new()
        .wallet(wallet)
        .on_http(rpc_url.parse()?);

    // Step 1: Instrument
    if !no_color {
        println!("\n{} Instrumenting contract...", "🧬".bright_cyan());
    } else {
        println!("\n[1/4] Instrumenting contract...");
    }

    let mut instrumentor = Instrumentor::new();
    let instrumented_code = instrumentor.instrument(source)?;
    fs::write(instrumented_output, &instrumented_code)?;

    if !no_color {
        println!(
            "   {} Probes injected: {}",
            "✓".bright_green(),
            instrumentor
                .get_instrumented_operations()
                .len()
                .to_string()
                .bright_yellow()
        );
    } else {
        println!(
            "   ✓ Probes injected: {}",
            instrumentor.get_instrumented_operations().len()
        );
    }

    // Step 2: Build WASM
    if !no_color {
        println!("\n{} Building WASM binary...", "⚙️".bright_cyan());
    } else {
        println!("\n[2/4] Building WASM binary...");
    }

    let temp_dir = TempDir::new()?;
    prepare_temp_project(original_file, instrumented_output, temp_dir.path())?;
    let wasm_path = build_wasm_in_temp(temp_dir.path())?;

    if !no_color {
        println!(
            "   {} WASM built → {}",
            "✓".bright_green(),
            wasm_path.display().to_string().dimmed()
        );
    } else {
        println!("   ✓ WASM built → {}", wasm_path.display());
    }

    // Step 3: Deploy
    if !no_color {
        println!("\n{} Deploying to chain...", "🚀".bright_cyan());
    } else {
        println!("\n[3/4] Deploying to chain...");
    }

    let bytecode = fs::read(&wasm_path)?;
    let deploy_req = TransactionRequest::default().input(bytecode.into());

    let pending_tx = provider.send_transaction(deploy_req).await?;
    let receipt = pending_tx
        .get_receipt()
        .await
        .context("Deployment receipt failed")?;

    let contract_addr = receipt
        .contract_address
        .context("No contract address - Stylus activation may be required")?;

    if !no_color {
        println!(
            "   {} Contract deployed at: {}",
            "✓".bright_green(),
            contract_addr.to_string().bright_yellow()
        );
    } else {
        println!("   ✓ Contract deployed at: {}", contract_addr);
    }

    // Step 4: Execute profiling transaction (if calldata provided)
    if let Some(calldata_hex) = &calldata {
        if !no_color {
            println!(
                "\n{} Executing profiling transaction...",
                "⚡".bright_cyan()
            );
        } else {
            println!("\n[4/4] Executing profiling transaction...");
        }

        let calldata_bytes = Bytes::from(hex::decode(calldata_hex.trim_start_matches("0x"))?);
        let value_wei = U256::from_str_radix(value, 10)?;

        let tx_req = TransactionRequest::default()
            .to(contract_addr)
            .value(value_wei)
            .input(calldata_bytes.into());

        let pending_tx = provider.send_transaction(tx_req).await?;
        let tx_receipt = pending_tx.get_receipt().await?;

        if !no_color {
            println!(
                "   {} Transaction mined → hash: {}",
                "✓".bright_green(),
                tx_receipt.transaction_hash.to_string().dimmed()
            );
        } else {
            println!(
                "   ✓ Transaction mined → hash: {}",
                tx_receipt.transaction_hash
            );
        }
    } else if !no_color {
        println!(
            "\n{} No --calldata provided, skipping execution",
            "⏭️".yellow()
        );
    } else {
        println!("\n[4/4] No --calldata provided, skipping execution");
    }

    // Step 5: Fetch and display ink report via get_ink_report()
    if !no_color {
        println!("\n{} Fetching ink report...", "📊".bright_cyan());
    } else {
        println!("\nFetching ink report...");
    }

    let selector = keccak256(b"get_ink_report()")[..4].to_vec();
    let call_tx = TransactionRequest::default()
        .to(contract_addr)
        .input(Bytes::from(selector).into());

    let raw_result = provider.call(&call_tx).await?;
    let report_str = String::from_utf8_lossy(&raw_result).to_string();

    if !no_color {
        println!("\n{}", "═".repeat(60).bright_cyan());
        println!("  {}", "💰 INK REPORT".bright_cyan().bold());
        println!("{}", "═".repeat(60).bright_cyan());
        println!("{}", report_str);
        println!("{}", "═".repeat(60).bright_cyan());
    } else {
        println!("\n═══════════════════════════════════════════");
        println!("  INK REPORT");
        println!("═══════════════════════════════════════════");
        println!("{}", report_str);
        println!("═══════════════════════════════════════════");
    }

    Ok(())
}

/// Prepares a temporary Cargo project with the instrumented source for building.
fn prepare_temp_project(original: &Path, instrumented: &Path, temp: &Path) -> Result<()> {
    let cargo_toml_src = original.parent().unwrap().join("Cargo.toml");
    if !cargo_toml_src.exists() {
        anyhow::bail!("Cargo.toml not found next to source file");
    }

    // Read original Cargo.toml
    let cargo_content = fs::read_to_string(&cargo_toml_src)?;

    // Parse as TOML (using toml crate you already have)
    let mut cargo_toml: toml::Value = cargo_content
        .parse()
        .context("Failed to parse original Cargo.toml")?;

    // Ensure [features] table exists
    if cargo_toml.get("features").is_none() {
        cargo_toml["features"] = toml::Value::Table(toml::map::Map::new());
    }

    // Add ink-profiling if not already present
    let features = cargo_toml
        .get_mut("features")
        .and_then(|f| f.as_table_mut())
        .context("Failed to access [features] table")?;

    if !features.contains_key("ink-profiling") {
        features.insert(
            "ink-profiling".to_string(),
            toml::Value::Array(vec![]), // empty array = just the feature flag
        );
        eprintln!(
            "{} Auto-added feature `ink-profiling` to temp Cargo.toml",
            "🔧".bright_cyan()
        );
    }

    // Write modified Cargo.toml to temp project
    fs::write(temp.join("Cargo.toml"), cargo_toml.to_string())?;

    fs::create_dir(temp.join("src"))?;
    fs::copy(instrumented, temp.join("src/lib.rs"))?;

    Ok(())
}

/// Extracts crate name from Cargo.toml (needed for locating the output .wasm)
fn get_crate_name_from_cargo_toml(cargo_toml_path: &Path) -> Result<String> {
    let content = fs::read_to_string(cargo_toml_path).context("Failed to read Cargo.toml")?;

    let value: Value = content
        .parse()
        .context("Failed to parse Cargo.toml as TOML")?;

    let name = value
        .get("package")
        .and_then(|pkg| pkg.get("name"))
        .and_then(|n| n.as_str())
        .context("No [package] name found in Cargo.toml")?
        .to_string();

    Ok(name)
}

/// Builds the WASM binary in a temporary directory using cargo +nightly.
fn build_wasm_in_temp(project: &Path) -> Result<PathBuf> {
    let output = Command::new("cargo")
        .current_dir(project)
        .args([
            "build",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "--features",
            "ink-profiling",
        ])
        .output()?;

    if !output.status.success() {
        anyhow::bail!(
            "cargo build failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let cargo_toml_path = project.join("Cargo.toml");
    let crate_name = get_crate_name_from_cargo_toml(&cargo_toml_path)?;

    let wasm = project.join(format!(
        "target/wasm32-unknown-unknown/release/{}.wasm",
        crate_name
    ));

    if !wasm.exists() {
        anyhow::bail!("Compiled WASM not found at: {}", wasm.display());
    }

    Ok(wasm)
}

/// Attempts to produce analyzable source code:
/// - If no sol! / Stylus macros detected → returns original
/// - Otherwise tries `cargo +nightly expand` in the project root
/// - Falls back to original source on failure
fn get_analyzable_source(original_source: &str, analyzed_file_abs: &Path) -> Result<String> {
    let source_dir = analyzed_file_abs
        .parent()
        .context("Analyzed file has no parent directory")?
        .to_path_buf();

    eprintln!(
        "{} Resolved source directory for expansion: {}",
        "📂".bright_blue(),
        source_dir.display()
    );

    let uses_sol_macros = original_source.contains("sol_storage!")
        || original_source.contains("sol_interface!")
        || original_source.contains("sol!")
        || original_source.contains("#[entrypoint]")
        || original_source.contains("#[public]");

    if !uses_sol_macros {
        eprintln!(
            "{} No sol!/Stylus macros detected → using original source",
            "ℹ️".bright_cyan()
        );
        return Ok(original_source.to_string());
    }

    eprintln!(
        "{} Detected sol! / Stylus macro usage → attempting automatic expansion",
        "ℹ️".bright_cyan()
    );

    let mut current = source_dir.clone();
    let mut found_root: Option<PathBuf> = None;

    loop {
        let cargo_path = current.join("Cargo.toml");
        eprintln!("  Checking: {}", cargo_path.display());

        if cargo_path.is_file() {
            found_root = Some(current.clone());
            eprintln!("  → Found Cargo.toml here: {}", current.display());
            break;
        }

        if !current.pop() {
            eprintln!("  Reached filesystem root → no Cargo.toml found");
            break;
        }
    }

    let contract_project_root = match found_root {
        Some(root) => root,
        None => {
            eprintln!(
                "{} No Cargo.toml found in parent directories → falling back to source directory",
                "⚠️".bright_yellow()
            );
            source_dir
        }
    };

    eprintln!(
        "{} Using project root for expansion: {}",
        "📍".bright_cyan(),
        contract_project_root.display()
    );

    let cargo_toml_path = contract_project_root.join("Cargo.toml");

    let is_lib_crate = if cargo_toml_path.is_file() {
        let content = fs::read_to_string(&cargo_toml_path).unwrap_or_default();
        content.contains("[lib]")
            || content.contains("crate-type")
            || content.contains("cdylib")
            || content.contains("wasm32-unknown-unknown")
    } else {
        false
    };

    let mut cmd = Command::new("cargo");
    cmd.arg("+nightly")
        .arg("expand")
        .current_dir(&contract_project_root);

    if is_lib_crate {
        cmd.arg("--lib");
        eprintln!(
            "{} Detected library crate → using --lib",
            "🧩".bright_magenta()
        );
    } else {
        cmd.arg("--bin");
        if cargo_toml_path.is_file() {
            if let Ok(name) = get_crate_name_from_cargo_toml(&cargo_toml_path) {
                cmd.arg(&name);
                eprintln!("{} Using --bin {}", "🧩".bright_magenta(), name);
            } else {
                eprintln!(
                    "{} Using --bin (no specific name found)",
                    "🧩".bright_magenta()
                );
            }
        } else {
            eprintln!(
                "{} Using --bin (no Cargo.toml → optimistic)",
                "🧩".bright_magenta()
            );
        }
    }

    let output = cmd
        .output()
        .context("Failed to execute cargo +nightly expand. Is rust nightly installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        eprintln!(
            "{} cargo expand failed in directory: {}\n{}\nFalling back to original source.",
            "⚠️".bright_red(),
            contract_project_root.display(),
            stderr
        );
        return Ok(original_source.to_string());
    }

    let expanded =
        String::from_utf8(output.stdout).context("cargo expand output is not valid UTF-8")?;

    let cleaned: String = expanded
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with("//")
        })
        .collect::<Vec<_>>()
        .join("\n");

    if cleaned.len() < 500 {
        eprintln!(
            "{} Expanded code seems suspiciously small ({} bytes) — falling back to original",
            "⚠️".bright_yellow(),
            cleaned.len()
        );
        return Ok(original_source.to_string());
    }

    eprintln!(
        "{} Successfully expanded code ({} bytes)",
        "✓".bright_green(),
        cleaned.len()
    );

    Ok(cleaned)
}
