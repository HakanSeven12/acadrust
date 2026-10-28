# Issue 135 AutoCAD Validation

Tested on 2026-10-21 with AutoCAD 2027 Core Console X.118.0.0, using the public
managed API and `AUDIT` with repairs disabled. All drawings are generated test
material; the reporter's private B, C, and D drawings are unavailable.

## Native Corpus

`Issue135Native.cs` creates 13 independent constraint groups with 60 constrained
geometries and 47 constraints. It covers lines, circles, arcs, ellipses,
elliptical arcs, splines, implicit endpoints, implicit centers, and datum lines.
Explicit geometric constraints are horizontal, vertical, parallel,
perpendicular, collinear, equal-length, concentric, equal-radius, tangent, and
fixed. AutoCAD also creates point-curve and center-point constraints.

The corpus is saved as AC1024 (2010), AC1027 (2013), and AC1032 (2018/current)
DWG plus ASCII DXF. Every native DWG reopens with the expected graph and zero
audit errors.

## Corrected Codec Results

| Input Route | 2010 | 2013 | 2018 |
| --- | --- | --- | --- |
| Native AutoCAD DWG | Pass | Pass | Pass |
| Native DWG -> Rust DWG | Pass | Pass | Pass |
| Native DWG -> Rust DXF | Pass | Pass | Pass |
| Native DWG -> Rust binary DXF | Pass | Pass | Pass |
| Native DWG -> Rust ASCII DXF -> Rust DWG | Pass | Pass | Pass |
| Native DWG -> Rust binary DXF -> Rust DWG | Pass | Pass | Pass |
| Rust output -> AutoCAD save -> reopen | Pass | Pass | Pass |
| Horizontal endpoint edit -> solver correction | Pass | Pass | Pass |

All 15 corrected outputs preserve the exact native inventory: group
handles, node IDs, geometry and constraint classes, active/implied flags,
degrees of freedom, and solution status. The full inventory remains unchanged
after evaluating the associative network. AutoCAD saves both DWG and DXF copies;
all 15 saved DWGs reopen, re-evaluate, and audit with zero errors.

All 15 outputs also pass a functional solver check: change the horizontal
line's endpoint from `(20, 0, 0)` to `(20, 5, 0)`, evaluate the network, and
require equal endpoint Y coordinates within `1e-8`. AutoCAD corrects the edit
and still audits with zero errors. This edit occurs after saving comparison
copies, so those copies retain the original geometry.

The validator disables entity record caches and invalidates the associative
source-record cache, so DWG writes exercise the codecs. Rust then strictly
reads all 30 AutoCAD-saved DWG/DXF copies and compares graph data with the native
R2018 oracle. ASCII rounding, the absent DXF registry flag, a raised node ID
seed, and AutoCAD's reordering of the owned-action set are the only allowed
normalizations. No source-record passthrough is used to claim writer coverage.

The public `2010/Constraints.dwg` reproducer initially failed with
`Invalid DWG array count 1073742597`. The patched reader preserves all nine real
nodes. All five conversion routes reopen with one group, five geometries,
four constraints, and zero audit errors.

## Failures Caught

The original Rust DXF output failed AutoCAD import for all three versions with
`Premature end of object` in `ACDBASSOC2DCONSTRAINTGROUP`. Correcting the subclass
boundary and the native R2010 inline node layout resolves those direct imports.
The original R2010 DXF-to-DWG result also failed to open. The matching inline
DXF reader correction and native inline DWG writer resolve that route. The
old direct DWG path copied unchanged associative source records, which masked
the writer defect until the validator disabled that cache.

## Rust Checks

| Check | Result |
| --- | --- |
| `cargo check --all-targets --all-features` | Pass |
| `cargo test --no-fail-fast` | 2,138 passed, 60 ignored, 0 failed |
| `cargo test --all-features --no-fail-fast` | 2,203 passed, 67 ignored, 0 failed |
| Source-only focused regressions, default and all features | 35 passed in each run, 0 failed |

The full-suite counts were recorded before converting the drawing fixtures to
Rust constructors; both focused runs were repeated after that test-only conversion.

`tests/support/issue135_graphs.rs` expresses the native graph values as typed
Rust constructors: 13 groups, 122 modeled nodes, and 20 node classes. The graph
values were compared field-for-field with all three native DWG versions before
excluding drawings from the patch. `issue135_stream.rs` independently encodes
the native main/text/handle streams and checks the complete decoded graphs.
Both constructed and independently decoded inputs traverse five codec routes
per version, for 30 Rust conversion routes. DXF pair regressions also directly
encode the affected layouts. These new regressions neither read nor embed drawings.

Additional tests cover sparse/empty groups, historical registry layouts,
unknown node classes, the reported invalid counts, bounded raw recovery, and
rejection of silent DXF loss. These source-only tests run by default; existing
skip annotations elsewhere remain in place.

## Limits

This validates the generated geometric corpus and public reproducer. It does
not establish compatibility with the reporter's unavailable private drawings
or every possible AutoCAD constraint configuration. Attempts to construct a
dimensional-constraint fixture through the installed AutoCAD managed API
terminated Core Console while attaching dimensional dependencies. Those
unsuccessful attempts are excluded from the passing corpus and reusable
generator. Dimensional constraints require separate native fixture coverage.

Command-based attempts were also unsuccessful in Core Console. `LINE` creates
a valid model-space entity, confirmed with `entget`. `DCALIGNED` with explicit
`Object` and an entity name exits without creating constraints; supplying a
pick point or an `(entity point)` pair reports `Invalid selection for Aligned`
and `No object found`. The prompt transcripts are retained locally in
`dimensions_explicit.log` and `dimensions_pair.log`.

## Reproduce

```powershell
./tests/support/issue135_autocad/Run.ps1 -Mode Generate
./tests/support/issue135_autocad/Run.ps1 -Mode Audit -Drawings output.dwg,output.dxf -ReferenceInventory tests/autocad/issue135/native_2010_dwg.inventory.txt -CheckHorizontalConstraint
```

Generated native files and complete logs live under ignored
`tests/autocad/issue135`. Corrected Rust outputs were validated from
`target/issue135-research/final-rust`, with final audit/evaluation logs in
`target/issue135-research/autocad-forced` and save/reopen logs in
`target/issue135-research/autocad-reopen`. Functional-edit logs are in
`target/issue135-research/autocad-functional`; all 15 contain
`ISSUE135_FUNCTIONAL_PASS`.
