#!/usr/bin/env bash
# Compare benchmark results across commits.
#
# Usage:
#   ./benchmarks/compare.sh                    # aggregate all commits
#   ./benchmarks/compare.sh 78f705d 4fe01ce    # compare two specific commits

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESULTS_FILE="${RESULTS_FILE:-$SCRIPT_DIR/results.jsonl}"

if [[ $# -ge 2 ]]; then
    FILTER="WHERE commit LIKE '${1}%' OR commit LIKE '${2}%'"
else
    FILTER=""
fi

duckdb -c "
WITH data AS (
  SELECT * FROM read_ndjson_auto('$RESULTS_FILE') $FILTER
)
SELECT
  commit[:7] AS commit,
  count(*) AS tests,
  sum(exec_instruction_count) AS Σinsns,
  sum(peak_heap_bytes) AS Σheap,
  sum(peak_stack_bytes) AS Σstack,
  sum(alloc_bytes_total) AS Σalloc,
  sum(exec_apply_count) AS Σapply,
  sum(exec_tail_apply_count) AS Σtail,
  sum(exec_match_count) AS Σmatch
FROM data
GROUP BY commit
ORDER BY min(timestamp);
"
