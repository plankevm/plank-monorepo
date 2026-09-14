# Stack scheduling strategy diary

This records strategies we tried, how successful they were, and whether we kept them.

| Strategy | Result | Kept |
| --- | --- | --- |
| Successor-only layout ordering | Generally the strongest layout-ordering variant. | Yes |
| Successor-plus-producer ordering | Mixed results and generally weaker than successor-only ordering. | Yes, for comparison |
| Alternative producer weights | Worse than equal successor and producer weighting. | No |
| Boundary global spilling | Improved all ten benchmark contracts. | No, superseded by persistent spilling |
| Full-relief call-argument spilling | Mostly neutral or regressive. | Yes, for reference |
| Partial call-argument spilling | Mostly regressive. | Yes, for reference |
| Ungated constant rematerialization | Caused severe bytecode regressions. | No |
| Cost-gated constant rematerialization | Reduced the ten-contract total by 1,739 bytes. | Yes |
| Bidirectional persistent spilling | Produced very large bytecode reductions, but could hoist stores onto paths that did not previously pay for them. | No, replaced with forward-only persistence |
| Forward persistent spilling | Reduced the ten-contract total by 486,650 bytes compared with boundary spilling. | Yes |
| Separately selectable boundary and persistent spilling | Added configuration complexity without a measured use for boundary spilling. | No |
| Single persistent dormant spilling | Current global spilling behavior. | Yes |
