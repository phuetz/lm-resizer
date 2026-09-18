use lm_resizer_core::ccr::{CcrStore, InMemoryCcrStore};
use lm_resizer_core::tokenizer::get_tokenizer;
use lm_resizer_core::transforms::source_compressor::{SourceCompressor, SourceLanguage};

#[test]
fn test_real_rust_file_measurements() {
    let source_path = "src/transforms/source_compressor.rs";
    let full_path = format!(
        "{}/crates/lm-resizer-core/{}",
        env!("CARGO_MANIFEST_DIR").trim_end_matches("/crates/lm-resizer-core"),
        source_path
    );
    let original = std::fs::read_to_string(&full_path).unwrap_or_else(|_| {
        std::fs::read_to_string("src/transforms/source_compressor.rs")
            .expect("must read source_compressor.rs")
    });

    let store = InMemoryCcrStore::default();
    let compressor = SourceCompressor::default();
    let res = compressor.compress_with_store(&original, Some(&store));

    assert_eq!(res.language, Some(SourceLanguage::Rust));
    assert!(res.omitted_body_lines > 0);
    assert!(res.omitted_functions > 0);
    assert!(res.ccr_key.is_some());

    // Verify CCR roundtrip
    let key = res.ccr_key.as_ref().unwrap();
    assert_eq!(store.get(key), Some(original.clone()));

    // Verify preservation of key signatures
    assert!(res.compressed.contains("pub fn compress_with_store"));
    assert!(res.compressed.contains("pub fn compress"));
    assert!(res.compressed.contains("pub fn detect_language"));
    assert!(res.compressed.contains("fn compress_rust"));
    assert!(res.compressed.contains("fn compress_python"));
    assert!(res.compressed.contains("fn compress_ts_js"));

    // Token measurements with tiktoken (gpt-4o / o200k)
    let tokenizer = get_tokenizer("gpt-4o");
    let orig_tokens = tokenizer.count_text(&original);
    let comp_tokens = tokenizer.count_text(&res.compressed);
    let token_savings = orig_tokens.saturating_sub(comp_tokens);
    let token_ratio = (comp_tokens as f64) / (orig_tokens as f64);

    eprintln!("\n=== RUST REAL FILE MEASUREMENTS ===");
    eprintln!("File: {}", source_path);
    eprintln!(
        "Original lines: {}, Compressed lines: {}",
        original.lines().count(),
        res.compressed.lines().count()
    );
    eprintln!(
        "Original bytes: {}, Compressed bytes: {} ({:.1}% reduction)",
        original.len(),
        res.compressed.len(),
        (1.0 - res.compressed.len() as f64 / original.len() as f64) * 100.0
    );
    eprintln!(
        "Original tokens: {}, Compressed tokens: {} ({:.1}% reduction)",
        orig_tokens,
        comp_tokens,
        (1.0 - token_ratio) * 100.0
    );
    eprintln!(
        "Omitted function bodies: {}, Omitted body lines: {}",
        res.omitted_functions, res.omitted_body_lines
    );
    eprintln!("CCR Hash: {}", key);

    assert!(token_savings > 0);
}

#[test]
fn test_real_typescript_measurements() {
    let original = r#"
import { Request, Response, NextFunction } from "express";
import { User, UserCredentials, SessionToken } from "./types/auth";
import { DatabaseService, ConnectionPool } from "./services/database";
import { AuditLogger, LogLevel } from "./services/audit";
import { MetricsCollector } from "./monitoring/metrics";

export interface AuthServiceConfig {
    jwtSecret: string;
    tokenExpirySeconds: number;
    maxFailedAttempts: number;
    lockoutDurationMinutes: number;
}

export interface AuthResult {
    success: boolean;
    user?: User;
    token?: SessionToken;
    errorMessage?: string;
}

export class AuthenticationManager {
    private readonly config: AuthServiceConfig;
    private readonly db: DatabaseService;
    private readonly logger: AuditLogger;

    constructor(config: AuthServiceConfig, db: DatabaseService, logger: AuditLogger) {
        this.config = config;
        this.db = db;
        this.logger = logger;
        this.validateConfiguration();
    }

    private validateConfiguration(): void {
        if (!this.config.jwtSecret || this.config.jwtSecret.length < 32) {
            throw new Error("JWT secret must be at least 32 characters long");
        }
        if (this.config.tokenExpirySeconds <= 0) {
            throw new Error("Token expiry must be positive");
        }
    }

    /**
     * Authenticate user with credentials and issue token.
     */
    public async authenticate(credentials: UserCredentials): Promise<AuthResult> {
        this.logger.log(LogLevel.INFO, `Attempting authentication for ${credentials.username}`);
        const user = await this.db.findUserByUsername(credentials.username);
        if (!user) {
            this.logger.log(LogLevel.WARN, `Unknown user: ${credentials.username}`);
            return { success: false, errorMessage: "Invalid credentials" };
        }

        const isValid = await this.verifyPassword(credentials.password, user.passwordHash);
        if (!isValid) {
            await this.recordFailedAttempt(user.id);
            return { success: false, errorMessage: "Invalid credentials" };
        }

        const token = await this.generateSessionToken(user);
        await this.db.storeSession(user.id, token);
        return { success: true, user, token };
    }

    private async verifyPassword(provided: string, storedHash: string): Promise<boolean> {
        const crypto = await import("crypto");
        const parts = storedHash.split(":");
        if (parts.length !== 2) return false;
        const [salt, hash] = parts;
        const derived = crypto.scryptSync(provided, salt, 64).toString("hex");
        return crypto.timingSafeEqual(Buffer.from(hash, "hex"), Buffer.from(derived, "hex"));
    }

    private async recordFailedAttempt(userId: string): Promise<void> {
        const attempts = await this.db.incrementFailedAttempts(userId);
        if (attempts >= this.config.maxFailedAttempts) {
            await this.db.lockAccount(userId, this.config.lockoutDurationMinutes);
        }
    }

    private async generateSessionToken(user: User): Promise<SessionToken> {
        const crypto = await import("crypto");
        const payload = {
            sub: user.id,
            role: user.role,
            exp: Math.floor(Date.now() / 1000) + this.config.tokenExpirySeconds,
        };
        const raw = JSON.stringify(payload);
        const signature = crypto.createHmac("sha256", this.config.jwtSecret).update(raw).digest("base64url");
        return { token: `${Buffer.from(raw).toString("base64url")}.${signature}`, expiresAt: payload.exp };
    }
}

export function authMiddleware(authMgr: AuthenticationManager) {
    return async (req: Request, res: Response, next: NextFunction) => {
        const authHeader = req.headers.authorization;
        if (!authHeader || !authHeader.startsWith("Bearer ")) {
            return res.status(401).json({ error: "Missing or malformed Authorization header" });
        }
        const token = authHeader.substring(7);
        try {
            const user = await authMgr.validateToken(token);
            (req as any).user = user;
            next();
        } catch (err) {
            return res.status(401).json({ error: "Invalid or expired token" });
        }
    };
}
"#;

    let store = InMemoryCcrStore::default();
    let compressor = SourceCompressor::default();
    let res = compressor.compress_with_store(original, Some(&store));

    assert_eq!(res.language, Some(SourceLanguage::TypeScript));
    assert!(res.omitted_body_lines > 0);
    assert!(res.omitted_functions >= 5);
    assert!(res.ccr_key.is_some());

    // Verify preservation of signatures and types
    assert!(res
        .compressed
        .contains("export interface AuthServiceConfig"));
    assert!(res.compressed.contains("export interface AuthResult"));
    assert!(res
        .compressed
        .contains("export class AuthenticationManager"));
    assert!(res
        .compressed
        .contains("public async authenticate(credentials: UserCredentials): Promise<AuthResult>"));
    assert!(res
        .compressed
        .contains("export function authMiddleware(authMgr: AuthenticationManager)"));

    let tokenizer = get_tokenizer("gpt-4o");
    let orig_tokens = tokenizer.count_text(original);
    let comp_tokens = tokenizer.count_text(&res.compressed);
    let token_ratio = (comp_tokens as f64) / (orig_tokens as f64);

    eprintln!("\n=== TYPESCRIPT REAL WORKLOAD MEASUREMENTS ===");
    eprintln!(
        "Original lines: {}, Compressed lines: {}",
        original.lines().count(),
        res.compressed.lines().count()
    );
    eprintln!(
        "Original bytes: {}, Compressed bytes: {} ({:.1}% reduction)",
        original.len(),
        res.compressed.len(),
        (1.0 - res.compressed.len() as f64 / original.len() as f64) * 100.0
    );
    eprintln!(
        "Original tokens: {}, Compressed tokens: {} ({:.1}% reduction)",
        orig_tokens,
        comp_tokens,
        (1.0 - token_ratio) * 100.0
    );
    eprintln!(
        "Omitted function bodies: {}, Omitted body lines: {}",
        res.omitted_functions, res.omitted_body_lines
    );
    eprintln!("CCR Hash: {}", res.ccr_key.unwrap());
}

#[test]
fn test_real_python_measurements() {
    let original = r#"
"""
Data pipeline ingestion, normalization, and aggregation service.
Handles streaming events and batches them to persistent data lake.
"""

import os
import sys
import time
import json
import logging
from typing import List, Dict, Any, Optional, Generator
from dataclasses import dataclass, field
from datetime import datetime, timezone

logger = logging.getLogger("pipeline.ingest")


@dataclass
class EventRecord:
    """Normalized analytical event representation."""
    event_id: str
    event_type: str
    payload: Dict[str, Any]
    timestamp: datetime = field(default_factory=lambda: datetime.now(timezone.utc))
    retry_count: int = 0


class IngestionPipeline:
    """Multi-stage stream processor for event ingestion."""

    def __init__(self, lake_path: str, batch_size: int = 1000, flush_interval_secs: float = 5.0):
        """Initialize pipeline with storage configuration."""
        self.lake_path = lake_path
        self.batch_size = batch_size
        self.flush_interval_secs = flush_interval_secs
        self._buffer: List[EventRecord] = []
        self._last_flush = time.monotonic()
        self._metrics = {"ingested": 0, "failed": 0, "flushed": 0}

    def push_event(self, raw_data: bytes) -> bool:
        """Validate, decode and push a raw payload into buffer."""
        try:
            parsed = json.loads(raw_data.decode("utf-8"))
            if "event_id" not in parsed or "event_type" not in parsed:
                self._metrics["failed"] += 1
                return False
            record = EventRecord(
                event_id=parsed["event_id"],
                event_type=parsed["event_type"],
                payload=parsed.get("data", {}),
            )
            self._buffer.append(record)
            self._metrics["ingested"] += 1
            if len(self._buffer) >= self.batch_size or (time.monotonic() - self._last_flush) >= self.flush_interval_secs:
                self.flush()
            return True
        except Exception as exc:
            logger.error("Failed to decode event: %s", exc)
            self._metrics["failed"] += 1
            return False

    def flush(self) -> int:
        """Flush buffered events to partitioned parquet/json storage."""
        if not self._buffer:
            return 0
        count = len(self._buffer)
        now_str = datetime.now(timezone.utc).strftime("%Y%m%d_%H%M%S")
        target_dir = os.path.join(self.lake_path, datetime.now(timezone.utc).strftime("year=%Y/month=%m"))
        os.makedirs(target_dir, exist_ok=True)
        file_path = os.path.join(target_dir, f"batch_{now_str}_{count}.jsonl")
        with open(file_path, "w", encoding="utf-8") as f:
            for rec in self._buffer:
                row = {
                    "event_id": rec.event_id,
                    "event_type": rec.event_type,
                    "payload": rec.payload,
                    "timestamp": rec.timestamp.isoformat(),
                }
                f.write(json.dumps(row) + "\n")
        self._buffer.clear()
        self._last_flush = time.monotonic()
        self._metrics["flushed"] += count
        logger.info("Flushed %d records to %s", count, file_path)
        return count

    def health_check(self) -> Dict[str, Any]:
        """Return system health diagnostics and metrics."""
        return {
            "status": "healthy" if self._metrics["failed"] < 100 else "degraded",
            "buffer_depth": len(self._buffer),
            "metrics": self._metrics.copy(),
            "lake_path": self.lake_path,
        }


def run_standalone_worker(storage_dir: str) -> None:
    """Main worker entry point for ingestion container."""
    pipeline = IngestionPipeline(storage_dir)
    logger.info("Worker started, monitoring input queue...")
    while True:
        time.sleep(1.0)
        pipeline.flush()


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO)
    data_dir = os.getenv("DATA_LAKE_PATH", "/tmp/lake")
    run_standalone_worker(data_dir)
"#;

    let store = InMemoryCcrStore::default();
    let compressor = SourceCompressor::default();
    let res = compressor.compress_with_store(original, Some(&store));

    assert_eq!(res.language, Some(SourceLanguage::Python));
    assert!(res.omitted_body_lines > 0);
    assert!(res.omitted_functions >= 5);
    assert!(res.ccr_key.is_some());

    // Verify preservation of signatures, types, and entry points
    assert!(res.compressed.contains("@dataclass"));
    assert!(res.compressed.contains("class EventRecord:"));
    assert!(res.compressed.contains("class IngestionPipeline:"));
    assert!(res.compressed.contains("def __init__(self, lake_path: str, batch_size: int = 1000, flush_interval_secs: float = 5.0):"));
    assert!(res
        .compressed
        .contains("def push_event(self, raw_data: bytes) -> bool:"));
    assert!(res.compressed.contains("def flush(self) -> int:"));
    assert!(res
        .compressed
        .contains("def health_check(self) -> Dict[str, Any]:"));
    assert!(res
        .compressed
        .contains("def run_standalone_worker(storage_dir: str) -> None:"));
    assert!(res.compressed.contains("if __name__ == \"__main__\":"));

    let tokenizer = get_tokenizer("gpt-4o");
    let orig_tokens = tokenizer.count_text(original);
    let comp_tokens = tokenizer.count_text(&res.compressed);
    let token_ratio = (comp_tokens as f64) / (orig_tokens as f64);

    eprintln!("\n=== PYTHON REAL WORKLOAD MEASUREMENTS ===");
    eprintln!(
        "Original lines: {}, Compressed lines: {}",
        original.lines().count(),
        res.compressed.lines().count()
    );
    eprintln!(
        "Original bytes: {}, Compressed bytes: {} ({:.1}% reduction)",
        original.len(),
        res.compressed.len(),
        (1.0 - res.compressed.len() as f64 / original.len() as f64) * 100.0
    );
    eprintln!(
        "Original tokens: {}, Compressed tokens: {} ({:.1}% reduction)",
        orig_tokens,
        comp_tokens,
        (1.0 - token_ratio) * 100.0
    );
    eprintln!(
        "Omitted function bodies: {}, Omitted body lines: {}",
        res.omitted_functions, res.omitted_body_lines
    );
    eprintln!("CCR Hash: {}", res.ccr_key.unwrap());
}

#[test]
fn test_all_required_edge_cases() {
    let compressor = SourceCompressor::default();

    // 1. Valid file of each language
    let rust_code = "pub fn add(a: i32, b: i32) -> i32 {\n    let s = a + b;\n    s\n}\n\npub fn sub(a: i32, b: i32) -> i32 {\n    let diff = a - b;\n    diff\n}\n";
    let rust_res = compressor.compress(rust_code);
    assert_eq!(rust_res.language, Some(SourceLanguage::Rust));

    let py_code = "def add(a: int, b: int) -> int:\n    res = a + b\n    return res\n\ndef sub(a: int, b: int) -> int:\n    res = a - b\n    return res\n";
    let py_res = compressor.compress(py_code);
    assert_eq!(py_res.language, Some(SourceLanguage::Python));

    let ts_code = "export function add(a: number, b: number): number {\n    const sum = a + b;\n    return sum;\n}\n\nexport function sub(a: number, b: number): number {\n    return a - b;\n}\n";
    let ts_res = compressor.compress(ts_code);
    assert!(
        ts_res.language == Some(SourceLanguage::TypeScript)
            || ts_res.language == Some(SourceLanguage::JavaScript)
    );

    // 2. Syntactically invalid files (must gracefully fallback, no panic, no empty output)
    let invalid_rust = "pub fn unclosed_brace(x: u32) {\n    let y = x * 2;\n// missing brace";
    let inv_rust_res = compressor.compress(invalid_rust);
    assert_eq!(inv_rust_res.language, None);
    assert!(!inv_rust_res.compressed.is_empty());
    assert!(!inv_rust_res.compressed.contains("Structure-compressed"));

    let invalid_ts = "export function unclosed(x: number) {\n    const y = x * 2;\n";
    let inv_ts_res = compressor.compress(invalid_ts);
    assert_eq!(inv_ts_res.language, None);
    assert!(!inv_ts_res.compressed.is_empty());

    // 3. Unknown language
    let bash_script = "#!/bin/bash\nset -euo pipefail\n# Run backup\necho \"Backing up...\"\ntar -czf backup.tar.gz /data\n";
    let bash_res = compressor.compress(bash_script);
    assert_eq!(bash_res.language, None);
    assert!(!bash_res.compressed.is_empty());

    // 4. Empty file
    let empty_res = compressor.compress("");
    assert_eq!(empty_res.compressed, "");

    // 6. Verify banner contains Structure approximative when without Code Explorer
    assert!(rust_res.is_approximate);
    assert!(rust_res.compressed.contains("Structure approximative"));
    assert!(!rust_res.compressed.contains("Code Explorer"));

    assert!(py_res.is_approximate);
    assert!(py_res.compressed.contains("Structure approximative"));

    assert!(ts_res.is_approximate);
    assert!(ts_res.compressed.contains("Structure approximative"));
}

#[test]
fn test_embedded_path_tested_extensively_without_code_explorer() {
    let compressor = SourceCompressor::embedded_only();
    let store = InMemoryCcrStore::default();

    // 1. Rust file test
    let rust_input = r#"
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn distance(&self, other: &Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }
}
"#;
    let rust_res = compressor.compress_with_store(rust_input, Some(&store));
    assert!(rust_res.is_approximate);
    assert_eq!(rust_res.engine_used, "embedded-regex-braces");
    assert!(rust_res.compressed.contains("// [Structure approximative:"));
    assert!(rust_res.ccr_key.is_some());
    let r_key = rust_res.ccr_key.unwrap();
    assert_eq!(store.get(&r_key), Some(rust_input.to_string()));

    // 2. Python file test
    let py_input = r#"
class Calculator:
    def add(self, a: int, b: int) -> int:
        temp = a + b
        return temp

    def multiply(self, a: int, b: int) -> int:
        prod = a * b
        return prod
"#;
    let py_res = compressor.compress_with_store(py_input, Some(&store));
    assert!(py_res.is_approximate);
    assert_eq!(py_res.engine_used, "embedded-regex-braces");
    assert!(py_res.compressed.contains("# [Structure approximative:"));
    assert!(py_res.ccr_key.is_some());
    let p_key = py_res.ccr_key.unwrap();
    assert_eq!(store.get(&p_key), Some(py_input.to_string()));
}

#[test]
fn test_code_explorer_ast_symbols_path_and_banner() {
    use lm_resizer_core::transforms::source_compressor::AstSymbol;

    let compressor = SourceCompressor::default();
    let store = InMemoryCcrStore::default();
    let input = r#"
pub fn calculate_hash(data: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for &b in data {
        h = (h ^ (b as u64)).wrapping_mul(0x100000001b3);
    }
    h
}
"#;
    let symbols = vec![AstSymbol {
        name: "calculate_hash".to_string(),
        label: "Function".to_string(),
        start_line: 2,
        end_line: 8,
    }];

    let res = compressor
        .compress_with_symbols(input, SourceLanguage::Rust, &symbols, Some(&store))
        .expect("ast symbols compression should succeed");

    assert!(!res.is_approximate);
    assert_eq!(res.engine_used, "code-explorer");
    assert!(res
        .compressed
        .contains("// [Structure syntaxique (Code Explorer):"));
    assert!(!res.compressed.contains("Structure approximative"));
    assert!(res
        .compressed
        .contains("calculate_hash(data: &[u8]) -> u64"));
    assert!(res
        .compressed
        .contains("/* ... [5 lines omitted: function body] ... */"));
    assert!(res.ccr_key.is_some());
    assert_eq!(
        store.get(res.ccr_key.as_ref().unwrap()),
        Some(input.to_string())
    );
}
