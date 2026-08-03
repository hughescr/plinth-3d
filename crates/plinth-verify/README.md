# plinth-verify

`plinth-verify` is the independent format gate for the artifact as written on
disk. It freshly reads the supplied path on every call, parses it with
`chitubox_io`, asks that crate to reserialize and run its independent structural
interval audit, and requires exact byte identity with the bytes just read.

A pass is structural self-consistency only. It never asserts that ChituBox's GUI
or native loader accepted the project; `GuiLoadabilityCheckpoint::NotAssessed`
is therefore reported unless separate native acceptance evidence is supplied by
a later checkpoint.

The report identifies the artifact with caller-provided `ArtifactId`, rather
than a machine-local filesystem path. Every failure emits exactly one blocking
`Defect` with measure `1.0`, whose unit is an artifact failure count: one failed
artifact/gate. `ArtifactUnreadable` names path/read I/O failures;
`ChituboxStructuralFormat` names parser, independent-audit/serialization, or
clean round-trip failures. Neither kind is `ChituboxLoadability`, which is
reserved exclusively for actual GUI/native acceptance evidence.
