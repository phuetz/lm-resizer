"""Independent oracle regression tests: fail when a required fact disappears."""
import unittest
from run import expand, missing

class OracleTests(unittest.TestCase):
    def test_exact_content_controls_and_references(self):
        view = 'LMR-LINES/2\n@"folder/"\na\nb\n&0\n=2\n@""\n\\@literal\n\\\\literal\nx\ry\n!0\n'
        self.assertEqual(expand(view), 'folder/a\nfolder/b\nfolder/a\nfolder/a\nfolder/a\n@literal\n\\literal\nx\ry')

    def test_missing_duplicate_grep_is_a_failure(self):
        case = dict(kind='grep', command=['grep', '-n', 'needle'])
        raw = 'a.rs:2: needle\na.rs:2: needle\n'
        required, lost = missing(case, raw, 'a.rs:2: needle\n')
        self.assertEqual(len(required), 2)
        self.assertEqual(len(lost), 1)

    def test_cat_final_newline_and_spaces_are_facts(self):
        case = dict(kind='cat', command=['cat', 'file'])
        self.assertTrue(missing(case, ' a\r\n', ' a\n')[1])
        self.assertTrue(missing(case, ' a\n', 'a\n')[1])
        self.assertTrue(missing(case, ' a\n', ' a')[1])
        raw = 'LMR-LINES/2\napplication output\n[raw: abc]\n'
        self.assertFalse(missing(case, raw, raw)[1])

    def test_empty_directory_and_last_error_must_survive(self):
        listing = dict(kind='recursive', command=['ls', '-R'])
        self.assertTrue(missing(listing, '.:\na\n\n./empty:\n', '.:\na\n')[1])
        tests = dict(kind='test', command=['pytest', '-q'])
        raw = 'ERROR first\nERROR last\n350 errors in 1.0s\n'
        self.assertEqual(len(missing(tests, raw, 'ERROR first\n')[1]), 2)

if __name__ == '__main__':
    unittest.main()
