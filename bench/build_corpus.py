#!/usr/bin/env python3
"""Generate the public, deterministic comparison corpus and its literal oracles."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
DATA = ROOT / "corpus"
DATA.mkdir(exist_ok=True)
CASES = []


def put(name, category, content, oracle, rtk=None, command=None):
    assert oracle and all(needle in content for needle in oracle), name
    path = DATA / f"{name}.txt"
    path.write_text(content, encoding="utf-8")
    CASES.append(dict(id=name, category=category, file=f"corpus/{name}.txt",
                      oracle=oracle, rtk_filter=rtk, command=command))


noise = lambda prefix, n: "".join(f"{prefix} {i:03d} ........................................ ok\n" for i in range(n))

put("cargo_ok", "tests", "   Compiling atlas-core v0.1.0\n" + noise("test parse::case", 80) +
    "test result: ok. 80 passed; 0 failed; 0 ignored; finished in 0.08s\n",
    ["80 passed", "0 failed"], "cargo-test", "cargo test")
put("cargo_fail", "tests", "   Compiling atlas-core v0.1.0\n" + noise("test parse::case", 70) +
    "test parse::reject_empty ... FAILED\n---- parse::reject_empty stdout ----\nthread 'parse::reject_empty' panicked at src/parser.rs:42:9:\nassertion `left == right` failed: expected=422 observed=200\nfailures:\n    parse::reject_empty\ntest result: FAILED. 70 passed; 1 failed; finished in 0.09s\n",
    ["parse::reject_empty", "src/parser.rs:42:9", "expected=422 observed=200", "1 failed"], "cargo-test", "cargo test")
put("dotnet_ok", "tests", "  Determining projects to restore...\n" + noise("Passed Atlas.Tests.Case", 65) +
    "Test Run Successful.\nTotal tests: 65\n     Passed: 65\n",
    ["Test Run Successful", "Passed: 65"], None, "dotnet test")
put("dotnet_fail", "tests", "  Determining projects to restore...\n" + noise("Passed Atlas.Tests.Case", 64) +
    "Failed Atlas.Tests.InvoiceTests.RejectsNegativeTotal [18 ms]\n  Error Message: Assert.Equal() Failure: Expected: 0 Actual: -14\n  Stack Trace: at Atlas.Tests.InvoiceTests.RejectsNegativeTotal() in tests/InvoiceTests.cs:line 87\nFailed: 1, Passed: 64, Total: 65\n",
    ["InvoiceTests.RejectsNegativeTotal", "Expected: 0 Actual: -14", "tests/InvoiceTests.cs:line 87", "Failed: 1"], None, "dotnet test")
put("npm_ok", "tests", "> atlas-web@1.0.0 test\n> vitest run\n" + noise("✓ src/cart.test.ts > case", 60) +
    " Test Files  1 passed (1)\n      Tests  60 passed (60)\n",
    ["1 passed (1)", "60 passed (60)"], None, "npm test")
put("npm_fail", "tests", "> atlas-web@1.0.0 test\n> vitest run\n" + noise("✓ src/cart.test.ts > case", 59) +
    " FAIL  src/cart.test.ts > Cart > retains tax\nAssertionError: expected 119 to be 120\n ❯ src/cart.test.ts:31:24\n Test Files  1 failed (1)\n      Tests  1 failed | 59 passed (60)\n",
    ["src/cart.test.ts:31:24", "retains tax", "expected 119 to be 120", "1 failed"], "vitest", "npm test")
put("pytest_ok", "tests", "============================= test session starts =============================\ncollected 70 items\n" +
    noise("tests/test_orders.py::test_case", 70) + "============================== 70 passed in 0.21s ==============================\n",
    ["70 passed"], "pytest", "pytest")
put("pytest_fail", "tests", "============================= test session starts =============================\ncollected 70 items\n" +
    noise("tests/test_orders.py::test_case", 69) + "tests/test_orders.py::test_reject_zero FAILED\n" +
    "________________________ test_reject_zero ________________________\n" +
    "E       AssertionError: expected status=422, got status=200\n" +
    "tests/test_orders.py:48: AssertionError\n" +
    "FAILED tests/test_orders.py::test_reject_zero - AssertionError\n" +
    "========================= 1 failed, 69 passed in 0.25s =========================\n",
    ["test_reject_zero", "tests/test_orders.py:48", "status=422", "status=200", "1 failed"], "pytest", "pytest")
put("git_diff", "git", "diff --git a/src/billing.py b/src/billing.py\nindex 1111111..2222222 100644\n--- a/src/billing.py\n+++ b/src/billing.py\n@@ -17,7 +17,7 @@ def total(items):\n" +
    " context\n" * 70 + "-    return subtotal + tax\n+    return round(subtotal + tax, 2)\n" +
    "diff --git a/tests/test_billing.py b/tests/test_billing.py\n@@ -40,3 +40,4 @@\n+def test_rounding(): assert total([1.005]) == 1.01\n",
    ["src/billing.py", "round(subtotal + tax, 2)", "tests/test_billing.py", "test_rounding"], "git-diff", "git diff")
put("git_log", "git", "commit aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nAuthor: Example Contributor <example@example.invalid>\n\n    Fix overflow in invoice totals\n" +
    "".join(f"commit {i:040x}\nAuthor: Example Contributor <example@example.invalid>\nDate: Tue Sep 22 12:00:00 2026 +0000\n\n    Maintenance batch {i}\n\n" for i in range(45)).rstrip() + "\n",
    ["Fix overflow in invoice totals", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"], "git-log", "git log")
put("docker", "infra", noise("Step build layer", 75) +
    "ERROR: failed to solve: process \"/bin/sh -c npm run build\" did not complete successfully: exit code: 1\nDockerfile:27\nimage: atlas-web:2026.09\n",
    ["ERROR: failed to solve", "exit code: 1", "Dockerfile:27", "atlas-web:2026.09"], None, "docker build")
put("psql", "database", "BEGIN\n" + noise("INSERT 0 1 row", 70) +
    "ERROR: duplicate key value violates unique constraint \"orders_pkey\"\nDETAIL: Key (id)=(4096) already exists.\nCONTEXT: SQL statement at migrations/042_orders.sql:19\nROLLBACK\n",
    ["orders_pkey", "(id)=(4096)", "migrations/042_orders.sql:19", "ROLLBACK"], None, "psql")
put("logs", "logs", noise("2026-09-22T12:00:00Z INFO worker processed batch", 85) +
    "2026-09-22T12:00:03Z ERROR payment reconciliation failed request_id=fixture-7 code=E_LEDGER_MISMATCH\n" +
    "2026-09-22T12:00:04Z WARN retry_exhausted attempts=3\n",
    ["request_id=fixture-7", "E_LEDGER_MISMATCH", "retry_exhausted attempts=3"], "log", "journalctl")
rows = [{"id": i, "state": "ok", "amount": i * 3, "meta": {"region": "test", "batch": i // 10}} for i in range(180)]
rows[143] = {"id": 143, "state": "rejected", "amount": 4299, "meta": {"region": "test", "reason": "limit_exceeded"}}
put("json_large", "json", json.dumps({"schema": "orders-v3", "count": 180, "rows": rows}, indent=2) + "\n",
    ['"schema": "orders-v3"', '"count": 180', '"id": 143', '"state": "rejected"', '"amount": 4299', '"reason": "limit_exceeded"'], None, "cat")
put("compile_error", "build", noise("Compiling module", 75) +
    "src/ledger.rs:73:18: error[E0308]: mismatched types\nexpected `i64`, found `String`\n" +
    "src/ledger.rs:91:5: warning: unused variable: `currency`\nerror: could not compile `atlas-ledger` due to 1 previous error\n",
    ["src/ledger.rs:73:18", "error[E0308]", "expected `i64`, found `String`", "atlas-ledger"], None, "cargo build")

code = {
    "csharp": ("public sealed class InvoiceService {\n    public decimal Total(IEnumerable<decimal> values) => values.Sum();\n" +
               "    public decimal ApplyTax(decimal total) => Math.Round(total * 1.20m, 2);\n" +
               "    public const string ErrorCode = \"INVOICE_NEGATIVE\";\n" + "    // illustrative helper\n" * 85 + "}\n",
               ["InvoiceService", "ApplyTax", "1.20m", "INVOICE_NEGATIVE"]),
    "rust": ("pub fn checked_total(values: &[i64]) -> Result<i64, TotalError> {\n    values.iter().try_fold(0, |sum, value| sum.checked_add(*value).ok_or(TotalError::Overflow))\n}\n" + "// illustrative helper\n" * 85,
             ["checked_total", "TotalError::Overflow", "checked_add"]),
    "python": ("def checked_total(values: list[int]) -> int:\n    result = sum(values)\n    if result > 10000:\n        raise ValueError('LIMIT_EXCEEDED')\n    return result\n" + "# illustrative helper\n" * 85,
               ["checked_total", "result > 10000", "LIMIT_EXCEEDED"]),
    "typescript": ("export function checkedTotal(values: number[]): number {\n  const total = values.reduce((a, b) => a + b, 0);\n  if (total > 10000) throw new Error('LIMIT_EXCEEDED');\n  return total;\n}\n" + "// illustrative helper\n" * 85,
                   ["checkedTotal", "values.reduce", "LIMIT_EXCEEDED"]),
    "go": ("func CheckedTotal(values []int) (int, error) {\n total := 0\n for _, value := range values { total += value }\n if total > 10000 { return 0, ErrLimitExceeded }\n return total, nil\n}\n" + "// illustrative helper\n" * 85,
           ["CheckedTotal", "total > 10000", "ErrLimitExceeded"]),
    "java": ("public final class InvoiceService {\n public long checkedTotal(long[] values) {\n  long total = 0;\n  for (long value : values) total = Math.addExact(total, value);\n  return total;\n }\n" + "// illustrative helper\n" * 85 + "}\n",
             ["InvoiceService", "checkedTotal", "Math.addExact"]),
}
for language, (body, oracle) in code.items():
    put("code_" + language, "code", body, oracle, None, "cat")

put("prose", "prose", "# Recovery procedure\n" +
    "The ledger is read only during reconciliation. An operator must inspect mismatch code E_LEDGER_MISMATCH before retrying.\n" +
    "Never retry more than three times; escalate to the on call owner after the third attempt.\n" +
    "The status page is updated only after the repair has passed the invoice checksum validation.\n" * 35,
    ["read only during reconciliation", "E_LEDGER_MISMATCH", "Never retry more than three times", "invoice checksum validation"], None, "cat")

(ROOT / "cases.json").write_text(json.dumps(CASES, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
print(f"{len(CASES)} cases written")
