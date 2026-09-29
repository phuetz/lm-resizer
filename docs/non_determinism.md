# Deterministic compression calculations

`HashMap` uses a random iteration order. When floating-point values are
accumulated in that order, the last bits of a result can change between runs.
The string entropy calculation in SmartCrusher had this problem. The
`entropy_is_bitwise_stable_across_repeated_maps` test reproduces it with the
previous `HashMap` and passes with the ordered `BTreeMap`.

The patch also uses ordered maps in BM25, anchor selection, and SmartCrusher
bucket indexing. In BM25, matched terms were already sorted before scoring;
the map change therefore does not by itself establish a previous output
variation. The other map changes have no demonstrated output regression test.

`bench/run_non_determinism_test.sh` compares two runs for each fixture in
`bench/cases.json` through `tool-output`, `compress`, and `exec`. The `exec`
route executes `cat` on a fixture; it does not execute the fixture's declared
command. A successful pairwise comparison is a smoke test, not a proof that
all inputs or the MCP/proxy paths are deterministic.
