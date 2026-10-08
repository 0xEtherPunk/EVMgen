mod clipboard;
mod engine;
mod evm;
mod solana;
mod ui;

use engine::{Chain, SearchConfig};
use evm::validate_evm_prefix;
use solana::validate_solana_prefix;
use ui::{execute_search, run_interactive_setup, TerminalGuard};

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        if args.iter().any(|a| a == "-h" || a == "--help") {
            println!("Vanity Wallet Address Generator (EVM & Solana)");
            println!("Usage:");
            println!("  Interactive mode:  evmgen");
            println!("  CLI mode:          evmgen --chain <evm|sol> --prefix <PREFIX> [OPTIONS]");
            println!();
            println!("Options:");
            println!("  --chain <evm|sol>   Target blockchain (default: evm)");
            println!("  --prefix <PREFIX>   Target prefix (hex for EVM, base58 for Solana)");
            println!("  --case-sensitive    Enable exact case matching");
            println!("  --threads <N>       Number of CPU worker threads (default: all cores)");
            println!("  -h, --help          Show this help message");
            return Ok(());
        }

        let mut chain = Chain::Evm;
        let mut prefix: Option<String> = None;
        let mut case_sensitive = false;
        let mut threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--chain" => {
                    if i + 1 < args.len() {
                        chain = match args[i + 1].to_lowercase().as_str() {
                            "sol" | "solana" => Chain::Solana,
                            _ => Chain::Evm,
                        };
                        i += 1;
                    }
                }
                "--prefix" => {
                    if i + 1 < args.len() {
                        prefix = Some(args[i + 1].clone());
                        i += 1;
                    }
                }
                "--case-sensitive" => {
                    case_sensitive = true;
                }
                "--threads" => {
                    if i + 1 < args.len() {
                        if let Ok(t) = args[i + 1].parse::<usize>() {
                            if t > 0 {
                                threads = t;
                            }
                        }
                        i += 1;
                    }
                }
                _ => {}
            }
            i += 1;
        }

        if let Some(p) = prefix {
            let valid_prefix = match chain {
                Chain::Evm => match validate_evm_prefix(&p) {
                    Ok(val) => val,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                },
                Chain::Solana => match validate_solana_prefix(&p) {
                    Ok(val) => val,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                },
            };

            let config = SearchConfig {
                chain,
                prefix: valid_prefix,
                case_sensitive,
                threads,
            };

            let _guard = TerminalGuard::new()?;
            return execute_search(config);
        }
    }

    // Default: Full interactive mode
    let _guard = TerminalGuard::new()?;
    if let Some(config) = run_interactive_setup()? {
        execute_search(config)?;
    }

    Ok(())
}
