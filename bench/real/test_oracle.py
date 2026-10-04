"""Independent oracle regression tests: fail when a required fact disappears."""
import unittest
from run import expand, missing

class OracleTests(unittest.TestCase):
    def test_exact_content_controls_and_references(self):
        view = 'LMR-LINES/2\n@"folder/"\na\nb\n&0\n=2\n@""\n\\@literal\n\\\\literal\nx\ry\n!0\n'
        self.assertEqual(expand(view), 'folder/a\nfolder/b\nfolder/a\nfolder/a\nfolder/a\n@literal\n\\literal\nx\ry')

    def test_text_dictionary_keeps_literal_references_and_line_endings(self):
        view = 'LMR-TEXT/1\n{"a":"longIdentifier"}\n~a ~~a é\r\n!0\n'
        self.assertEqual(expand(view), 'longIdentifier ~a é\r')
        self.assertEqual(expand(view.replace('!0', '!1')), 'longIdentifier ~a é\r\n')

    def test_version3_prefix_dictionary_and_blocks(self):
        view = 'LMR-LINES/3\n@"folder/"\na\nb\n@0:"nested/"\nc\n@0\nd\n&0:3\n!0\n'
        self.assertEqual(expand(view), 'folder/a\nfolder/b\nfolder/nested/c\nfolder/d\nfolder/a\nfolder/b\nfolder/nested/c')
        for final in ('!0', '!1'):
            self.assertEqual(expand('LMR-LINES/3\nx\n'+final+'\n'), 'x'+('\n' if final == '!1' else ''))

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

class VisibleMutationTests(unittest.TestCase):
    """Each mutant round-trips through the old decoder yet lies in the view."""
    def reject_mutant(self, kind, raw, mutant):
        case = dict(kind=kind, command=[{'grep':'grep', 'log':'git', 'cat':'cat', 'ordered':'docker'}[kind]])
        self.assertFalse(missing(case, raw, raw)[1])
        self.assertEqual(expand(mutant), raw)
        self.assertTrue(missing(case, raw, mutant)[1])

    def test_B1_number_prefix_mutation(self):
        self.reject_mutant('grep', 'src/file.ts:179:const invoice = 12030;\nsrc/file.ts:181:missing_identifier;\n',
            'LMR-LINES/3\n@"src/file.ts:1"\n79:const invoice = 12030;\n81:missing_identifier;\n!1\n')

    def test_B2_author_reference_mutation(self):
        a, b, c = 'a'*40, 'b'*40, 'c'*40
        raw = f'commit {a}\nAuthor: Alice\nDate: day one\n\ncommit {b}\nAuthor: Bob\nDate: day two\n\ncommit {c}\nAuthor: Alice\nDate: day one\n'
        mutant = f'LMR-LINES/3\ncommit {a}\nAuthor: Alice\nDate: day one\n\ncommit {b}\nAuthor: Bob\nDate: day two\n\ncommit {c}\n&1:2\n!1\n'
        self.reject_mutant('log', raw, mutant)
        # A global multiset also misses authors swapped between commits.
        swapped = raw.replace('Author: Alice', 'Author: TEMP', 1).replace('Author: Bob', 'Author: Alice', 1).replace('Author: TEMP', 'Author: Bob')
        self.assertTrue(missing(dict(kind='log', command=['git']), raw, swapped)[1])

    def test_B3_literal_dictionary_mutation(self):
        self.reject_mutant('cat', 'HOME_CFG = "~/.config/app"\n    return ~long_identifier\n',
            'LMR-TEXT/1\n{"a":"long_identifier"}\nHOME_CFG = "~~/.config/app"\n    return ~~~a\n!1\n')

    def test_B4_error_block_mutation(self):
        self.reject_mutant('ordered', 'ERROR database refused\ncheckpoint\nERROR database refused\nexited 137\n',
            'LMR-LINES/3\nERROR database refused\ncheckpoint\n&0\nexited 137\n!1\n')

if __name__ == '__main__':
    unittest.main()
