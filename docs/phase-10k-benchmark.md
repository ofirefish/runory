# Phase 10K deterministic benchmark

This is a deterministic fixture estimate, not a wall-clock production measurement. It compares the Phase 10H/10J five-pack plans under the previous full-context/sequential-read assumptions and the Phase 10K projection/dedup/cache/parallel-read rules.

| Incident pack | Tool calls before | Tool calls after | Context tokens before | Context tokens after | Critical path before (ms) | Critical path after (ms) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Website | 8 | 7 | 22,000 | 6,200 | 8,000 | 3,000 |
| Nginx | 7 | 6 | 18,000 | 5,200 | 7,000 | 2,800 |
| Docker | 7 | 6 | 19,000 | 5,600 | 7,500 | 3,000 |
| Disk Full | 8 | 6 | 24,000 | 6,800 | 9,000 | 3,400 |
| Linux Service | 7 | 5 | 17,000 | 4,900 | 6,500 | 2,600 |
| Total | 37 | 30 | 100,000 | 28,700 | 38,000 | 14,800 |

Estimated aggregate reductions are 18.9% tool calls, 71.3% context tokens, and 61.1% read critical path. Cost improvement follows input-token reduction for providers that report token pricing; the local deterministic provider reports zero estimated cost.

Every fixture asserts unchanged diagnosis correctness, exact approval safety, verification coverage, and audit coverage. Precondition and verification calls are deliberately excluded from dedup/cache savings.
