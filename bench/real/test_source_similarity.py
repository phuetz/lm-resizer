#!/usr/bin/env python3
"""Regression tests for the source-clone barrier, including the reviewed clone."""
import re
import unittest
from check_source_similarity import ROOT, LIMIT, compare, functions, oracle_sources, policy_matches, small_functions


class SourceBarrierTests(unittest.TestCase):
    def test_renamed_reviewed_function_is_rejected(self):
        upstream, _ = oracle_sources(ROOT/'target/rtk-parity/source.tar.gz')
        source = upstream['src/cmds/git/diff_cmd.rs']
        start = source.index('fn parse_hunk_header(')
        end = source.index('\n}', start) + 2
        original = source[start:end]
        renamed = re.sub(r'\b(header|parts|old|new|range|count|start|prefix|parse_range|parse_hunk_header)\b',
                         lambda m: 'renamed_' + m[0], original)
        findings = compare({'sample.rs': renamed}, {'oracle.rs': original})
        self.assertGreaterEqual(max(m['tokens'] for m in findings), LIMIT)

    def test_production_after_test_module_is_still_scanned(self):
        source = '#[cfg(test)] mod tests { fn fixture() { unreachable!(); } }\nfn live() { println!("yes"); }'
        self.assertEqual([name for name, _, _ in functions(source)], ['live'])

    def test_test_expectations_and_declarations_do_not_trigger(self):
        declarations = 'struct Config { ' + 'pub name: String,\n' * 40 + '}'
        test = '#[test] fn fixture() { ' + 'assert_eq!(sample(), "value");' * 20 + '}'
        self.assertEqual(compare({'a.rs': declarations + test}, {'b.rs': declarations + test}), [])

    def test_literals_comments_and_names_do_not_defeat_alarm(self):
        original = 'fn first() {' + ''.join(f'let x{i} = input.read().unwrap_or({i}); output.push(x{i});' for i in range(8)) + '}'
        changed = original.replace('input', 'bytes').replace('output', 'result').replace('first', 'second')
        changed = re.sub(r'\b[0-9]+\b', '99', changed).replace(';', '; /* comment */')
        self.assertTrue(any(m['tokens'] >= LIMIT for m in compare({'a.rs': changed}, {'b.rs': original})))

    def test_split_reordered_truncation_policy_is_rejected(self):
        first = 'fn a() { let limit = 100; let context = 3; let other = 4; input.take(limit).take(context); println!("... (more changes truncated)"); println!("{} rows omitted", limit); }'
        second = 'fn renamed() { println!("... (more changes truncated)"); } fn helper() { let x = 4; let z = 100; let y = 3; input.skip(z).min(y); println!("{} lines truncated", z); }'
        self.assertTrue(policy_matches({'local.rs': second}, {'oracle.rs': first}))
        # Shared public vocabulary or numeric idioms alone is insufficient.
        self.assertFalse(policy_matches({'local.rs': second.replace('truncated', 'kept')}, {'oracle.rs': first}))
        self.assertFalse(policy_matches({'local.rs': second.replace('100', '12')}, {'oracle.rs': first}))
        self.assertFalse(policy_matches({'local.rs': '#[cfg(test)] mod tests {' + second + '}'}, {'oracle.rs': first}))

    def test_three_line_function_copy_is_rejected_without_matching_other_apis(self):
        upstream, _ = oracle_sources(ROOT/'target/rtk-parity/source.tar.gz')
        source = upstream['src/cmds/git/git_cmd.rs']
        start = source.index('fn tokenize_git_log_args(')
        original = source[start:source.index('\n}', start)+2]
        renamed = original.replace('tokenize_git_log_args', 'small_copy').replace('args', 'input')
        matches = small_functions({'copy.rs': renamed}, {'oracle.rs': original})
        self.assertEqual(len(matches), 1)
        self.assertEqual(matches[0]['tokens'], 15)
        # The same short syntactic idiom calling different APIs is not a clone.
        different = renamed.replace('arg_tokenizer', 'independent_parser')
        self.assertFalse(small_functions({'own.rs': different}, {'oracle.rs': original}))
        self.assertFalse(small_functions({'test.rs': '#[cfg(test)] mod tests {'+renamed+'}'}, {'oracle.rs': original}))

    def test_mismatched_archive_is_refused(self):
        import tempfile
        from pathlib import Path
        with tempfile.TemporaryDirectory() as tmp:
            archive = Path(tmp)/'bad.tar.gz'
            archive.write_bytes(b'not the verified archive')
            with self.assertRaisesRegex(ValueError, 'checksum'):
                oracle_sources(archive)


if __name__ == '__main__':
    unittest.main()
