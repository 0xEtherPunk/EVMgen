use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::evm::{
    format_evm_result, matches_prefix_case_insensitive, matches_prefix_case_sensitive,
    EvmGenerator, EvmResult,
};
use crate::solana::{
    format_solana_result, matches_prefix_case_insensitive as solana_matches_insensitive,
    matches_prefix_case_sensitive as solana_matches_sensitive, SolanaGenerator, SolanaResult,
};

pub enum Chain {
    Evm,
    Solana,
}

pub enum MatchedResult {
    Evm(EvmResult),
    Solana(SolanaResult),
}

pub struct SearchConfig {
    pub chain: Chain,
    pub prefix: String,
    pub case_sensitive: bool,
    pub threads: usize,
}

pub struct SearchEngine {
    config: SearchConfig,
    stop_flag: Arc<AtomicBool>,
    total_checked: Arc<AtomicU64>,
    result: Arc<Mutex<Option<MatchedResult>>>,
}

impl SearchEngine {
    pub fn new(config: SearchConfig) -> Self {
        Self {
            config,
            stop_flag: Arc::new(AtomicBool::new(false)),
            total_checked: Arc::new(AtomicU64::new(0)),
            result: Arc::new(Mutex::new(None)),
        }
    }

    pub fn stop_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stop_flag)
    }

    pub fn total_checked(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.total_checked)
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }

    pub fn start_workers(&self) -> Vec<thread::JoinHandle<()>> {
        let mut handles = Vec::with_capacity(self.config.threads);
        const BATCH_SIZE: u64 = 512;

        for _ in 0..self.config.threads {
            let stop = Arc::clone(&self.stop_flag);
            let counter = Arc::clone(&self.total_checked);
            let result_store = Arc::clone(&self.result);
            let prefix = self.config.prefix.clone();
            let case_sensitive = self.config.case_sensitive;

            match self.config.chain {
                Chain::Evm => {
                    let prefix_lower = prefix.to_ascii_lowercase();
                    let handle = thread::spawn(move || {
                        let mut gen = EvmGenerator::new(2048);
                        let mut local_batch = 0u64;

                        while !stop.load(Ordering::Relaxed) {
                            let address = gen.next_address();
                            local_batch += 1;

                            let matched = if case_sensitive {
                                matches_prefix_case_sensitive(&address, &prefix, &prefix_lower)
                            } else {
                                matches_prefix_case_insensitive(&address, &prefix_lower)
                            };

                            if matched {
                                stop.store(true, Ordering::SeqCst);
                                let secret_key = gen.current_secret_key();
                                let res = format_evm_result(secret_key, address);
                                let mut lock = result_store.lock().unwrap();
                                *lock = Some(MatchedResult::Evm(res));
                                counter.fetch_add(local_batch, Ordering::Relaxed);
                                return;
                            }

                            gen.step();

                            if local_batch >= BATCH_SIZE {
                                counter.fetch_add(local_batch, Ordering::Relaxed);
                                local_batch = 0;
                            }
                        }

                        if local_batch > 0 {
                            counter.fetch_add(local_batch, Ordering::Relaxed);
                        }
                    });
                    handles.push(handle);
                }
                Chain::Solana => {
                    let prefix_lower = prefix.to_ascii_lowercase();
                    let handle = thread::spawn(move || {
                        let mut local_batch = 0u64;

                        while !stop.load(Ordering::Relaxed) {
                            let (signing_key, pubkey_bytes) = SolanaGenerator::generate_raw();
                            local_batch += 1;

                            let matched = if case_sensitive {
                                solana_matches_sensitive(&pubkey_bytes, &prefix)
                            } else {
                                solana_matches_insensitive(&pubkey_bytes, &prefix_lower)
                            };

                            if matched {
                                stop.store(true, Ordering::SeqCst);
                                let res = format_solana_result(signing_key, pubkey_bytes);
                                let mut lock = result_store.lock().unwrap();
                                *lock = Some(MatchedResult::Solana(res));
                                counter.fetch_add(local_batch, Ordering::Relaxed);
                                return;
                            }

                            if local_batch >= BATCH_SIZE {
                                counter.fetch_add(local_batch, Ordering::Relaxed);
                                local_batch = 0;
                            }
                        }

                        if local_batch > 0 {
                            counter.fetch_add(local_batch, Ordering::Relaxed);
                        }
                    });
                    handles.push(handle);
                }
            }
        }

        handles
    }

    pub fn take_result(&self) -> Option<MatchedResult> {
        self.result.lock().unwrap().take()
    }
}
