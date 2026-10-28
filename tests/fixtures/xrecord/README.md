These are byte-aligned XRECORD payload samples from native AutoCAD 2027
drawings saved as DWG R2000 and R2018, rather than files produced by acadrust.
Each fixture concatenates the captured `Databytes` of the individual probe
records P5, P105, P320, P329, P330, P390, P70, P289, P290, P310, P10 and P40,
in that order. Concatenation is valid because the payload is a sequence of
group-code/value pairs (ODA specification section 20.4.104).

The source captures were `opencode/reB/xr2000_dump.txt` and
`opencode/reB/xr2018_P.txt`, generated from `xr.lsp`: `entmakex` creates one
XRECORD for each code with `abc`, the named object dictionary entity name,
integer 7, point `(1,2,3)`, or hexadecimal chunk `AB`, as appropriate.
The drawings were saved by AutoCAD using `SAVEAS` for each target version.
The probe extracted the raw payload before interpreting its value types.

Codes 5, 105 and 320-329 contain the text `abc`, despite their handle-related
meaning in other records. R2000 strings contain a two-byte byte count,
one-byte code-page index 0x21, and text bytes. R2018 strings contain a
two-byte UTF-16 code-unit count and UTF-16LE text. Codes 330 and 390 instead
contain the eight-byte integer 12. Code 290 retains the native byte 7,
which decodes as true and must remain byte-for-byte intact when transcoding.

These samples establish the tested values and version transition; they do
not establish every accepted group code or every private application schema.
