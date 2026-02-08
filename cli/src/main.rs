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
use reporter::Reporter;

#[derive(Parser)]
#[command(name = "inkwell")]
#[command(about = "🧪 Inkwell - Dive deep into Stylus contract gas analysis")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 🔬 Dip into your contract - analyze ink consumption patterns
    #[command(alias = "d")]
    Dip {
        /// Path to the Rust contract file (usually src/lib.rs)
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Target specific function for analysis
        #[arg(short, long)]
        function: Option<String>,

        /// Output format: compact, detailed, json
        #[arg(short, long, default_value = "compact")]
        output: String,

        /// Ink threshold for highlighting operations
        #[arg(long, default_value = "100000")]
        threshold: u64,

        /// Disable colored output
        #[arg(long)]
        no_color: bool,

        /// Run real ink profiling on-chain (requires --rpc-url and --private-key)
        #[arg(short, long)]
        profile: bool,

        /// RPC endpoint for profiling mode
        #[arg(long, default_value = "http://localhost:8547")]
        rpc_url: String,

        /// Private key for deploying and calling contract (profiling mode only)
        #[arg(long)]
        private_key: Option<String>,

        /// Chain ID for transactions (profiling mode only)
        #[arg(long, default_value = "1337")]
        chain_id: u64,

        /// 0x-prefixed hex calldata for profiling transaction
        #[arg(long)]
        calldata: Option<String>,

        /// Value to send with profiling transaction (in wei)
        #[arg(long, default_value = "0")]
        value: String,

        /// Save instrumented code to this file (profiling mode only)
        #[arg(long, default_value = "instrumented_contract.rs")]
        instrumented_output: PathBuf,
    },

    /// 🧬 Instrument your contract with ink tracking probes
    #[command(alias = "i")]
    Instrument {
        /// Path to the Rust contract file
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Output path for instrumented contract
        #[arg(short, long, default_value = "instrumented_contract.rs")]
        output: PathBuf,

        /// Disable colored output
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
                run_analysis_mode(&source, function.as_deref(), &output, threshold, no_color)?;
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

fn run_analysis_mode(
    source: &str,
    function: Option<&str>,
    output_format: &str,
    threshold: u64,
    no_color: bool,
) -> Result<()> {
    let analysis = analyze_contract(source, function)?;
    let reporter = Reporter::new(output_format, threshold, !no_color);
    reporter.print_report(&analysis)?;

    // Save JSON report
    fs::write("ink-report.json", serde_json::to_string_pretty(&analysis)?)?;

    // Generate VS Code decorations
    let vscode_dir = PathBuf::from(".inkwell");
    fs::create_dir_all(&vscode_dir)?;
    let decorations = reporter.generate_vscode_decorations(&analysis)?;
    fs::write(
        vscode_dir.join("decorations.json"),
        serde_json::to_string_pretty(&decorations)?,
    )?;

    if !no_color {
        println!("\n{}", "━".repeat(60).dimmed());
        println!("{} Reports saved:", "📊".bright_cyan());
        println!("   • ink-report.json");
        println!("   • .inkwell/decorations.json");
        println!(
            "\n{} Try {} for runtime profiling",
            "💡".bright_yellow(),
            "inkwell dip --profile".bright_white().bold()
        );
    } else {
        println!("\nReports saved:");
        println!("   • ink-report.json");
        println!("   • .inkwell/decorations.json");
        println!("\nTip: Use --profile for real ink measurement");
    }

    Ok(())
}

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
    let signer = PrivateKeySigner::from_slice(&pk_bytes)?.with_chain_id(Some(chain_id.into()));
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

    // Step 4: Execute profiling transaction
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
    } else {
        if !no_color {
            println!(
                "\n{} No --calldata provided, skipping execution",
                "⏭️".yellow()
            );
        } else {
            println!("\n[4/4] No --calldata provided, skipping execution");
        }
    }

    // Step 5: Fetch ink report
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

fn prepare_temp_project(original: &Path, instrumented: &Path, temp: &Path) -> Result<()> {
    let cargo_toml_src = original.parent().unwrap().join("Cargo.toml");
    if !cargo_toml_src.exists() {
        anyhow::bail!("Cargo.toml not found next to source file");
    }
    fs::copy(cargo_toml_src, temp.join("Cargo.toml"))?;
    fs::create_dir(temp.join("src"))?;
    fs::copy(instrumented, temp.join("src/lib.rs"))?;
    Ok(())
}

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
