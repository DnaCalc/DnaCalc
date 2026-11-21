# W009 review metadata fixture

Readable `.xlsx` package parts for structured notes, threaded comment storage,
threaded-comment people, VML comment evidence, and defined-name metadata.

Sheet 1 is intentionally complex: a rich legacy note, a threaded-comment
placeholder, VML linkage, threaded reply topology, mentions, and extension
fragments. These records are read and preserved but unsafe to rewrite in W009.

Sheet 2 is intentionally simple: one existing legacy note and one existing
threaded comment that can be edited by the W009 source-backed save policy.

OxDoc models file records and package/storage boundaries only. It does not own
conversation behavior, identities, mention semantics, VML geometry editing, UI
display, or defined-name binding semantics.
