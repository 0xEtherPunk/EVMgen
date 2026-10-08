use std::io::{stdout, IsTerminal, Stdout, Write};
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType};
use crossterm::{execute, queue};

use crate::engine::{Chain, MatchedResult, SearchConfig, SearchEngine};
use crate::evm::validate_evm_prefix;
use crate::solana::validate_solana_prefix;

pub struct TerminalGuard {
    is_tty: bool,
}

impl TerminalGuard {
    pub fn new() -> std::io::Result<Self> {
        let is_tty = std::io::stdin().is_terminal();
        if is_tty {
            terminal::enable_raw_mode()?;
            let mut out = stdout();
            let _ = execute!(out, cursor::Hide);
        }
        Ok(Self { is_tty })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.is_tty {
            let _ = terminal::disable_raw_mode();
            let mut out = stdout();
            let _ = execute!(out, cursor::Show, ResetColor);
        }
    }
}

pub fn run_interactive_setup() -> std::io::Result<Option<SearchConfig>> {
    let mut chain = Chain::Evm;
    let mut case_sensitive = false;
    let mut prefix = String::new();
    let mut error_msg: Option<String> = None;

    let available_threads = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let mut out = stdout();
    render_menu(&mut out, &chain, case_sensitive, &prefix, &error_msg)?;

    loop {
        if let Event::Key(key_event) = event::read()? {
            if key_event.modifiers.contains(KeyModifiers::CONTROL)
                && key_event.code == KeyCode::Char('c')
            {
                return Ok(None);
            }

            match key_event.code {
                KeyCode::Esc => {
                    return Ok(None);
                }
                KeyCode::Char('q') | KeyCode::Char('Q') if prefix.is_empty() => {
                    return Ok(None);
                }
                KeyCode::Char('/') => {
                    chain = match chain {
                        Chain::Evm => Chain::Solana,
                        Chain::Solana => Chain::Evm,
                    };
                    error_msg = None;
                }
                KeyCode::Tab => {
                    case_sensitive = !case_sensitive;
                }
                KeyCode::Backspace => {
                    prefix.pop();
                    error_msg = None;
                }
                KeyCode::Enter => {
                    let validated = match chain {
                        Chain::Evm => validate_evm_prefix(&prefix).map_err(|e| e.to_string()),
                        Chain::Solana => validate_solana_prefix(&prefix),
                    };

                    match validated {
                        Ok(valid_prefix) => {
                            return Ok(Some(SearchConfig {
                                chain,
                                prefix: valid_prefix,
                                case_sensitive,
                                threads: available_threads,
                            }));
                        }
                        Err(e) => {
                            error_msg = Some(e);
                        }
                    }
                }
                KeyCode::Char(c) => {
                    if !key_event.modifiers.contains(KeyModifiers::CONTROL)
                        && !key_event.modifiers.contains(KeyModifiers::ALT)
                    {
                        prefix.push(c);
                        error_msg = None;
                    }
                }
                _ => {}
            }

            render_menu(&mut out, &chain, case_sensitive, &prefix, &error_msg)?;
        }
    }
}

fn render_menu(
    out: &mut Stdout,
    chain: &Chain,
    case_sensitive: bool,
    prefix: &str,
    error: &Option<String>,
) -> std::io::Result<()> {
    queue!(out, terminal::Clear(ClearType::All), cursor::MoveTo(0, 0))?;

    queue!(
        out,
        SetForegroundColor(Color::Cyan),
        Print("========================================================================\r\n"),
        Print("                    VANITY WALLET ADDRESS GENERATOR                     \r\n"),
        Print("========================================================================\r\n"),
        ResetColor,
        SetForegroundColor(Color::DarkGrey),
        Print(" Hotkeys:\r\n"),
        Print("   [/]   Switch Chain (EVM <-> Solana)\r\n"),
        Print("   [Tab] Toggle Case Sensitivity (OFF <-> ON)\r\n"),
        Print("   [q]   Quit (when prefix empty) / [Esc] / [Ctrl+C]\r\n"),
        Print("------------------------------------------------------------------------\r\n"),
        ResetColor
    )?;

    let chain_label = match chain {
        Chain::Evm => "EVM (0x... Ethereum, BSC, Polygon, Arbitrum, Base)",
        Chain::Solana => "Solana (Base58 address)",
    };

    queue!(
        out,
        Print(" Chain:          "),
        SetForegroundColor(Color::Yellow),
        Print(chain_label),
        ResetColor,
        Print("\r\n")
    )?;

    let case_label = if case_sensitive {
        ("ON  (Exact match)", Color::Green)
    } else {
        ("OFF (Case-insensitive)", Color::DarkGrey)
    };

    queue!(
        out,
        Print(" Case-Sensitive: "),
        SetForegroundColor(case_label.1),
        Print(case_label.0),
        ResetColor,
        Print("\r\n")
    )?;

    let prefix_prompt = match chain {
        Chain::Evm => "0x",
        Chain::Solana => "",
    };

    queue!(
        out,
        Print("------------------------------------------------------------------------\r\n"),
        Print(" Desired Prefix: > "),
        SetForegroundColor(Color::Cyan),
        Print(prefix_prompt),
        SetForegroundColor(Color::White),
        Print(prefix),
        ResetColor,
        Print("\r\n")
    )?;

    if let Some(err) = error {
        queue!(
            out,
            SetForegroundColor(Color::Red),
            Print(format!(" [!] Error: {}\r\n", err)),
            ResetColor
        )?;
    } else {
        queue!(out, Print("\r\n"))?;
    }

    queue!(
        out,
        SetForegroundColor(Color::DarkGrey),
        Print(" Press [Enter] to start generation...\r\n"),
        ResetColor
    )?;

    out.flush()?;
    Ok(())
}

pub fn execute_search(config: SearchConfig) -> std::io::Result<()> {
    let mut out = stdout();
    execute!(out, terminal::Clear(ClearType::All), cursor::MoveTo(0, 0))?;

    let chain_name = match config.chain {
        Chain::Evm => "EVM",
        Chain::Solana => "Solana",
    };

    let display_prefix = match config.chain {
        Chain::Evm => format!("0x{}", config.prefix),
        Chain::Solana => config.prefix.clone(),
    };

    execute!(
        out,
        SetForegroundColor(Color::Cyan),
        Print("========================================================================\r\n"),
        Print("                         SEARCH IN PROGRESS                             \r\n"),
        Print("========================================================================\r\n"),
        ResetColor,
        Print(format!(" Chain:          {}\r\n", chain_name)),
        Print(format!(" Target Prefix:  {}\r\n", display_prefix)),
        Print(format!(
            " Case-Sensitive: {}\r\n",
            if config.case_sensitive { "YES" } else { "NO" }
        )),
        Print(format!(" Active Threads: {}\r\n", config.threads)),
        SetForegroundColor(Color::DarkGrey),
        Print(" Press [q] or [Esc] to abort\r\n"),
        Print("------------------------------------------------------------------------\r\n"),
        ResetColor
    )?;

    let engine = SearchEngine::new(config);
    let total_counter = engine.total_checked();
    let stop_flag = engine.stop_flag();
    let handles = engine.start_workers();

    let start_time = Instant::now();
    let mut last_sample_time = Instant::now();
    let mut last_sample_count = 0u64;

    loop {
        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Esc
                    || key.code == KeyCode::Char('q')
                    || key.code == KeyCode::Char('Q')
                    || (key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c'))
                {
                    engine.stop();
                    for h in handles {
                        let _ = h.join();
                    }
                    execute!(
                        out,
                        Print("\r\n"),
                        SetForegroundColor(Color::Yellow),
                        Print(" Search aborted by user. Memory scrubbed.\r\n"),
                        ResetColor
                    )?;
                    return Ok(());
                }
            }
        }

        let current_count = total_counter.load(Ordering::Relaxed);
        let elapsed = start_time.elapsed();
        let sample_elapsed = last_sample_time.elapsed().as_secs_f64();

        let speed = if sample_elapsed >= 0.2 {
            let count_diff = current_count.saturating_sub(last_sample_count);
            let s = count_diff as f64 / sample_elapsed;
            last_sample_time = Instant::now();
            last_sample_count = current_count;
            s
        } else {
            0.0
        };

        let hours = elapsed.as_secs() / 3600;
        let mins = (elapsed.as_secs() % 3600) / 60;
        let secs = elapsed.as_secs() % 60;

        let speed_str = format_speed(speed);
        let total_str = format_number(current_count);

        queue!(
            out,
            cursor::MoveToColumn(0),
            Clear(ClearType::CurrentLine),
            SetForegroundColor(Color::Green),
            Print(" [RUNNING] "),
            ResetColor,
            Print(format!(
                "Speed: {:<14} | Tested: {:<12} | Elapsed: {:02}:{:02}:{:02}",
                speed_str, total_str, hours, mins, secs
            ))
        )?;
        out.flush()?;

        if stop_flag.load(Ordering::Relaxed) {
            break;
        }
    }

    for h in handles {
        let _ = h.join();
    }

    let elapsed = start_time.elapsed();
    let total_tested = total_counter.load(Ordering::Relaxed);
    let matched = engine.take_result();

    execute!(
        out,
        Print("\r\n\r\n"),
        SetForegroundColor(Color::Green),
        Print("========================================================================\r\n"),
        Print("                          MATCH FOUND!                                  \r\n"),
        Print("========================================================================\r\n"),
        ResetColor
    )?;

    let copyable_key = match &matched {
        Some(MatchedResult::Evm(res)) => Some(res.private_key_hex.clone()),
        Some(MatchedResult::Solana(res)) => Some(res.private_key_bs58.clone()),
        None => None,
    };

    let copyable_addr = match &matched {
        Some(MatchedResult::Evm(res)) => Some(res.address.clone()),
        Some(MatchedResult::Solana(res)) => Some(res.address.clone()),
        None => None,
    };

    match &matched {
        Some(MatchedResult::Evm(res)) => {
            execute!(
                out,
                Print(" Chain:            "),
                SetForegroundColor(Color::Yellow),
                Print("EVM\r\n"),
                ResetColor,
                Print(" Address:          "),
                SetForegroundColor(Color::Cyan),
                Print(format!("{}\r\n", res.address)),
                ResetColor,
                Print(" Private Key:      "),
                SetForegroundColor(Color::White),
                Print(format!("{}\r\n", res.private_key_hex)),
                ResetColor,
                Print(format!(
                    " Performance:      {} keys in {:02}:{:02}:{:02}\r\n",
                    format_number(total_tested),
                    elapsed.as_secs() / 3600,
                    (elapsed.as_secs() % 3600) / 60,
                    elapsed.as_secs() % 60
                )),
                SetForegroundColor(Color::DarkGrey),
                Print("------------------------------------------------------------------------\r\n"),
                ResetColor,
                Print(" Controls: ["),
                SetForegroundColor(Color::Green),
                Print("y"),
                ResetColor,
                Print("] Copy Key | ["),
                SetForegroundColor(Color::Cyan),
                Print("a"),
                ResetColor,
                Print("] Copy Address | ["),
                SetForegroundColor(Color::Red),
                Print("q"),
                ResetColor,
                Print("] Exit & Wipe\r\n"),
                ResetColor
            )?;
        }
        Some(MatchedResult::Solana(res)) => {
            execute!(
                out,
                Print(" Chain:            "),
                SetForegroundColor(Color::Yellow),
                Print("Solana\r\n"),
                ResetColor,
                Print(" Address:          "),
                SetForegroundColor(Color::Cyan),
                Print(format!("{}\r\n", res.address)),
                ResetColor,
                Print(" Private Key (B58): "),
                SetForegroundColor(Color::White),
                Print(format!("{}\r\n", res.private_key_bs58)),
                ResetColor,
                Print(" Keypair JSON:     "),
                SetForegroundColor(Color::DarkGrey),
                Print(format!("{}\r\n", res.keypair_bytes_json)),
                ResetColor,
                Print(format!(
                    " Performance:      {} keys in {:02}:{:02}:{:02}\r\n",
                    format_number(total_tested),
                    elapsed.as_secs() / 3600,
                    (elapsed.as_secs() % 3600) / 60,
                    elapsed.as_secs() % 60
                )),
                SetForegroundColor(Color::DarkGrey),
                Print("------------------------------------------------------------------------\r\n"),
                ResetColor,
                Print(" Controls: ["),
                SetForegroundColor(Color::Green),
                Print("y"),
                ResetColor,
                Print("] Copy Key | ["),
                SetForegroundColor(Color::Cyan),
                Print("a"),
                ResetColor,
                Print("] Copy Address | ["),
                SetForegroundColor(Color::Red),
                Print("q"),
                ResetColor,
                Print("] Exit & Wipe\r\n"),
                ResetColor
            )?;
        }
        None => {
            execute!(
                out,
                SetForegroundColor(Color::Yellow),
                Print(" Search terminated with no match.\r\n"),
                ResetColor
            )?;
        }
    }

    if std::io::stdin().is_terminal() {
        loop {
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if key.code == KeyCode::Char('q')
                        || key.code == KeyCode::Char('Q')
                        || key.code == KeyCode::Esc
                        || (key.modifiers.contains(KeyModifiers::CONTROL)
                            && key.code == KeyCode::Char('c'))
                    {
                        break;
                    }

                    if (key.code == KeyCode::Char('y') || key.code == KeyCode::Char('Y'))
                        && copyable_key.is_some()
                    {
                        if let Some(ref text) = copyable_key {
                            match crate::clipboard::copy_text(text) {
                                Ok(_) => {
                                    execute!(
                                        out,
                                        SetForegroundColor(Color::Green),
                                        Print(" [✔] Private key copied to clipboard!\r\n"),
                                        ResetColor
                                    )?;
                                }
                                Err(e) => {
                                    execute!(
                                        out,
                                        SetForegroundColor(Color::Red),
                                        Print(format!(" [!] Clipboard error: {}\r\n", e)),
                                        ResetColor
                                    )?;
                                }
                            }
                        }
                    }

                    if (key.code == KeyCode::Char('a') || key.code == KeyCode::Char('A'))
                        && copyable_addr.is_some()
                    {
                        if let Some(ref addr) = copyable_addr {
                            match crate::clipboard::copy_text(addr) {
                                Ok(_) => {
                                    execute!(
                                        out,
                                        SetForegroundColor(Color::Cyan),
                                        Print(" [✔] Address copied to clipboard!\r\n"),
                                        ResetColor
                                    )?;
                                }
                                Err(e) => {
                                    execute!(
                                        out,
                                        SetForegroundColor(Color::Red),
                                        Print(format!(" [!] Clipboard error: {}\r\n", e)),
                                        ResetColor
                                    )?;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    execute!(
        out,
        Print("\r\n"),
        SetForegroundColor(Color::Green),
        Print(" Memory securely wiped (zeroized). Exiting.\r\n"),
        ResetColor
    )?;

    Ok(())
}

fn format_speed(speed: f64) -> String {
    if speed >= 1_000_000.0 {
        format!("{:.2} M keys/s", speed / 1_000_000.0)
    } else if speed >= 1_000.0 {
        format!("{:.1} k keys/s", speed / 1_000.0)
    } else {
        format!("{:.0} keys/s", speed)
    }
}

fn format_number(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let len = s.len();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result
}
