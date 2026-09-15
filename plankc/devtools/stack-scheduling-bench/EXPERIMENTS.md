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
| Cost-guided shared layout search | Global spilling + rematerialization | 2.748% | 6/0/4 | 21.733% | 46.034% | Improved 10/10 alone, but increased the current total by 17,268 bytes and cost about 2.8-3.7x compiler CPU. | No |
| Edge-specific layouts | Original baseline | n/a | 0/0/10 | 13.813% | 18.213% | Increased total size by 129,154 bytes. | No |
| Direct tail-call elimination | Original baseline | 0.034% | 9/1/0 | n/a | n/a | Saved 154 bytes. | No, superseded |
| Tail-call elimination through forwarders | Original baseline | 0.042% | 9/1/0 | n/a | n/a | Saved 265 bytes; saved 249 bytes on the current global pipeline. | Yes |
| Memory-backed return destinations | Global spilling + rematerialization | 0.834% | 6/4/0 | n/a | n/a | Saved 1,052 bytes; a targeted two-call benchmark saved 269 runtime gas. | Yes |

## Not kept

- **Alternative producer weights:** Performed worse than equal successor and producer weighting.
- **Boundary global spilling:** Superseded by persistent global spilling.
- **Ungated constant rematerialization:** Regressed all ten benchmark contracts.
- **Bidirectional persistent spilling:** Could hoist stores onto paths that previously avoided them;
  replaced by forward-only persistence.
- **Cost-guided shared layout search:** Strong alone, but expensive and regressed the current global
  spilling configuration.
- **Edge-specific layouts:** Adapter jump and shuffle overhead regressed every benchmark contract.
- **Direct tail-call elimination:** Superseded by tail-call elimination through empty forwarders.
