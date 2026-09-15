# Stack scheduling strategy diary

This records strategies we tried, how successful they were, and whether we kept them. Average
improvement includes only contracts that improved. `I/E/R` means improved, equal, and regressed.
Average and worst regression include only contracts that regressed.

| Strategy | Compared with | Avg improvement | I/E/R | Avg regression | Worst regression | Outcome | Kept |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- |
| Successor-only layout ordering | Naive ordering | Not recorded | Not recorded | Not recorded | Not recorded | Generally the strongest layout-ordering variant. | Yes |
| Successor-plus-producer ordering | Successor-only ordering | Not recorded | Not recorded | Not recorded | Not recorded | Mixed results and generally weaker than successor-only ordering. | Yes, for comparison |
| Alternative producer weights | Equal successor and producer weighting | Not recorded | Not recorded | Not recorded | Not recorded | Worse than equal weighting. | No |
| Boundary global spilling | Original baseline | 9.915% | 10/0/0 | n/a | n/a | Saved 125,634 bytes. | No, superseded |
| Full-relief call-argument spilling | Original baseline | 0.030% | 2/2/6 | 0.193% | 0.398% | Increased total size by 353 bytes. | Yes, for reference |
| Partial call-argument spilling | Original baseline | 0.307% | 1/0/9 | 1.161% | 2.316% | Increased total size by 3,984 bytes. | Yes, for reference |
| Ungated constant rematerialization | Original baseline | n/a | 0/0/10 | 6.007% | 18.956% | Increased total size by 99,567 bytes. | No |
| Cost-gated call-only constant rematerialization | Original baseline | 0.327% | 9/1/0 | n/a | n/a | Saved 1,739 bytes. | Yes |
| Bidirectional persistent spilling | Boundary spilling | 41.052% | 10/0/0 | n/a | n/a | Saved 500,289 bytes, but could hoist stores backward. | No |
| Global spilling | Original baseline | 43.712% | 10/0/0 | n/a | n/a | Saved 612,284 bytes. | Yes |
| Global spilling + rematerialization | Original baseline | 45.345% | 10/0/0 | n/a | n/a | Saved 623,866 bytes, including 11,582 bytes beyond global spilling alone. | Yes |
