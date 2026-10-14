# Executable task order (one valid topological ordering)

This is not a schedule. Independent ready tasks may run in parallel. Epics are grouping records.

1. `infra-m0.1` — Create the monorepo and isolated crate workspaces
2. `infra-m0.3` — Build the acceptance runner and architecture gates
3. `infra-m0.4` — Pin legacy sources and resolve inaccessible migration inputs
4. `infra-m0.2` — Install Windows Linux and browser CI
5. `infra-m0.5` — Quarantine questionable numeric provenance
6. `infra-m0.6` — Back up and content-address existing oracle evidence
7. `cfunc-m0.1` — Define portable values IDs and numerical policy
8. `cdoc-m0.1` — Define engine-neutral authored operations and revisions
9. `cfunc-m1.1` — Generate the initial function catalogue and call interface
10. `cfml-m1.1` — Port the lossless Excel lexer and basic parser
11. `infra-m1.1` — Unify cache and live-oracle verdicts
12. `cdoc-m1.1` — Implement bounded ZIP package reading and raw preservation
13. `ceng-m1.1` — Create workbook-wide graph storage and literal input access
14. `cfunc-m1.2` — Port base coercions and scalar arithmetic
15. `cfunc-m3.22` — Declare numerical sensitivity and generate backend routing
16. `cfml-m1.2` — Implement typed reference binding without the engine
17. `infra-m1.2` — Implement one isolated Excel COM capture adapter
18. `infra-m7.1` — Define licensed corpus manifests and stratified coverage
19. `cdoc-m1.2` — Load scalar cells formulas and file-cache provenance
20. `cfunc-m3.1` — Port text and Unicode functions
21. `cfunc-m3.2` — Port numeric rounding and integer functions
22. `cfunc-m3.4` — Port aggregate and counting functions
23. `cfunc-m3.5` — Port conditional aggregates and criteria matching
24. `cfunc-m3.6` — Port lookup and reference-returning functions
25. `cfunc-m3.7` — Port date and time functions
26. `cfunc-m3.8` — Port business-calendar functions
27. `cfunc-m3.15` — Port database and filtering aggregates
28. `cfunc-m3.16` — Port dynamic array transformation functions
29. `cfunc-m3.18` — Port information and host-context functions
30. `cfunc-m3.19` — Port volatile and provider-backed function adapters
31. `cfunc-m3.20` — Port modern text and locale-sensitive catalogue additions
32. `cfunc-m3.21` — Port remaining logical and compatibility aliases
33. `cfunc-m3.23` — Implement the deterministic portable Fast backend
34. `cfunc-m3.24` — Implement a reviewed native Exact instruction adapter
35. `cfunc-m4.1` — Expose a source-neutral mutable UDF registry
36. `cfunc-m7.1` — Measure a portable extended-precision arithmetic candidate
37. `cfml-m1.3` — Expose reusable compiled plans and evaluation frames
38. `infra-m5.1` — Capture iterative and data-table scheduling witnesses
39. `infra-m5.2` — Capture CF DV and precision-as-displayed witnesses
40. `infra-m7.3` — Acquire public corpus candidates with provenance review
41. `cdoc-m1.3` — Author cell changes and write a repair-free thin workbook
42. `cfunc-m3.3` — Port transcendental and trigonometric adapters
43. `cfunc-m3.9` — Port distribution functions in bounded dispatch batches
44. `cfunc-m3.10` — Port descriptive statistics and ranking functions
45. `cfunc-m3.11` — Port regression correlation and forecast functions
46. `cfunc-m3.12` — Port financial cash-flow functions
47. `cfunc-m3.13` — Port securities and depreciation functions
48. `cfunc-m3.14` — Port complex and engineering functions
49. `ceng-m1.2` — Calculate the thin slice through CalcFml
50. `onecalc-m1.1` — Ship the engine-free OneCalc application controller
51. `cfml-m2.1` — Canonicalize R1C1 templates and binding-context keys
52. `cfml-m2.6` — Own number-format parsing and numeric text policy
53. `cdoc-m2.1` — Persist view manifests and stable locator names
54. `cdoc-m3.4` — Preserve external links richData and cell metadata
55. `cdoc-m3.6` — Model styles themes rows columns and panes
56. `chost-m1.1` — Commit authored and engine edits as one transaction
57. `cfml-m2.2` — Invalidate reusable plans by semantic dependencies
58. `cfml-m2.3` — Compile LET LAMBDA closures and invocation
59. `cfml-m2.5` — Expose incremental editing and formula-language services
60. `cdoc-m3.1` — Read and write workbook and sheet defined names
61. `cdoc-m5.2` — Model conditional-format rules and formula anchors
62. `cdoc-m5.3` — Model data-validation rules and input messages
63. `cdoc-m6.4` — Preserve macro projects signatures and package extensions
64. `cdoc-m6.1` — Author complete cell formats and style tables
65. `cdoc-m6.2` — Author row column outline and pane metadata
66. `chost-m1.2` — Expose open recalc save and delta projections
67. `ceng-m2.1` — Instantiate shared formula regions without a second evaluator
68. `cfml-m3.1` — Bind scoped and caller-relative defined names
69. `cfml-m2.4` — Port metadata-driven lazy control forms
70. `cfunc-m3.17` — Port callable array helpers
71. `onecalc-m2.1` — Add OneCalc editing traces formats and callable fixtures
72. `cdoc-m3.2` — Read and write structured table definitions
73. `infra-m1.3` — Prove the first real workbook round trip
74. `chost-m2.1` — Define capability-organized skin intents and subscriptions
75. `cfml-m3.2` — Bind structured tables and 3D sheet spans
76. `cfml-m3.4` — Preserve array evaluation and legacy array contexts
77. `cfunc-m3.25` — Audit complete catalogue migration and generated outputs
78. `cdoc-m3.5` — Model chart drawing pivot and page-setup attachment data
79. `chost-m2.2` — Implement canonical commands and undo journal
80. `dnacalc-m2.1` — Build shared shell and desktop/browser transport
81. `cfml-m3.3` — Report dynamic references and reference algebra reads
82. `cfml-m3.5` — Bind external workbook references through a typed catalogue
83. `cdoc-m3.3` — Preserve shared array and dynamic-array formula encodings
84. `cfml-m6.1` — Complete number-format sections locales and numeric parsing
85. `cdoc-m6.3` — Model comments notes and hyperlinks
86. `chost-m2.3` — Implement block editing on ordinary worksheet anchors
87. `infra-m3.1` — Capture and reconcile the complete reference Excel UI inventory
88. `dnacalc-m2.5` — Implement the Excel skin minimum grid and formula bar
89. `ceng-m3.1` — Index ranges whole axes and absence dependencies
90. `cfml-m4.1` — Transform references for copy fill move and paste
91. `cfml-m4.3` — Bind UDF generations and asynchronous result tokens
92. `cdoc-m4.1` — Implement persistent sparse pages and axis order maps
93. `cdoc-m5.1` — Model and author what-if data-table formulas
94. `chost-m2.4` — Reconcile non-Excel layouts after real Excel edits
95. `dnacalc-m2.2` — Implement the notebook skin editor
96. `dnacalc-m2.3` — Implement the tree editor skin
97. `dnacalc-m2.4` — Implement the free-form canvas skin
98. `dnacalc-m6.2` — Implement range multi-area and axis selection
99. `ceng-m3.2` — Schedule scoped name instances and forward lookups
100. `ceng-m3.3` — Update dynamic read overlays atomically
101. `cfml-m4.2` — Transform references for cell axis and sheet lifecycle edits
102. `dnacalc-m2.6` — Show formula details and dependency navigation in every skin
103. `ceng-m3.5` — Calculate sheet spans and table-structure dependencies
104. `chost-m3.3` — Implement canonical named-function block editing
105. `ceng-m3.4` — Implement spill allocation and blocker invalidation
106. `ceng-m3.6` — Replay volatile time and random effects per tick
107. `ceng-m3.7` — Resolve external workbook snapshots and link updates
108. `cdoc-m4.2` — Prepare reversible cell range and axis patches
109. `cfml-m5.1` — Compile per-cell rule contexts for CF and DV
110. `cfml-m6.2` — Complete editor reference modes and diagnostics
111. `infra-m2.1` — Prove cross-skin xlsx persistence with Excel edits
112. `ceng-m3.8` — Attach generic derived participants to the workbook graph
113. `ceng-m3.9` — Unify connected open workbooks in one calculation domain
114. `cdoc-m4.3` — Author sheet lifecycle and relationship changes
115. `ceng-m4.1` — Apply structural patches without rebuild-from-input
116. `ceng-m5.6` — Register conditional-formatting dependency participants
117. `ceng-m5.8` — Evaluate validation candidates in staged input contexts
118. `dnacalc-m6.3` — Implement edit enter point and IME modes
119. `onecalc-m6.1` — Complete OneCalc function exploration and fixture persistence
120. `chost-m3.1` — Publish calculation generations and object revisions
121. `ceng-m4.6` — Apply external RTD and async completion operations
122. `cdoc-m6.6` — Complete source-preserving package authoring and Save As
123. `ceng-m4.2` — Implement calculation modes and recalc command scopes
124. `ceng-m4.5` — Publish counted escalation facts and replay traces
125. `ceng-m5.1` — Detect and maintain workbook strongly connected components
126. `ceng-m5.7` — Evaluate color scales data bars and icon sets
127. `chost-m3.2` — Expose generic refresh and render attachment interfaces
128. `chost-m4.1` — Prepare one atomic operation stream for structural edits
129. `dnacalc-m3.1` — Complete named-callable and spill inspection across all skins
130. `cdoc-m6.7` — Enforce transactional save output and lazy source access
131. `ceng-m4.3` — Apply date precision and calcPr policies
132. `ceng-m5.4` — Evaluate one-variable data tables with input overlays
133. `ceng-m5.2` — Implement captured iterative scheduling and convergence
134. `infra-m3.20` — Plan charting and drawing objects implementation
135. `infra-m3.21` — Plan pivot tables caches and getpivotdata implementation
136. `infra-m3.22` — Plan printing page layout and export implementation
137. `infra-m3.23` — Plan external data connections and analytical integrations implementation
138. `infra-m3.24` — Plan extended desktop command and dialog surface implementation
139. `chost-m4.2` — Implement core range editing and clipboard commands
140. `chost-m4.3` — Implement sheet name table and workbook lifecycle commands
141. `chost-m4.5` — Implement core cell formatting and style commands
142. `chost-m4.9` — Complete bounded undo redo and save snapshot retention
143. `infra-m3.3` — Complete the advanced skin persistence interchange matrix
144. `ceng-m4.4` — Invalidate all numeric results on mode change
145. `cdoc-m6.5` — Write calcPr calcChain and trustworthy formula caches
146. `ceng-m5.3` — Handle iterative arrays dynamic topology and failure publication
147. `ceng-m5.5` — Evaluate two-variable data tables and cyclic table semantics
148. `ceng-m5.9` — Calculate cycles crossing linked open workbooks
149. `chost-m4.4` — Implement sort filter and visibility commands
150. `chost-m4.6` — Implement find replace and navigation query commands
151. `chost-m5.3` — Implement validation authoring prompts and rejected edits
152. `dnacalc-m6.4` — Implement fill handle and AutoFill series
153. `dnacalc-m6.5` — Implement drag move copy and insert gestures
154. `dnacalc-m6.6` — Implement clipboard and Paste Special UI
155. `chost-m6.2` — Implement comments notes hyperlinks and review commands
156. `dnacalc-m6.11` — Implement Name Manager and name-box navigation
157. `dnacalc-m6.17` — Implement insert delete cells rows columns and sheets UI
158. `chost-m4.7` — Implement workbook protection and permission checks
159. `chost-m5.2` — Implement CF rule authoring and projection commands
160. `chost-m6.1` — Implement panes dimensions grouping and sheet-view commands
161. `dnacalc-m6.8` — Implement Format Cells Number and Alignment
162. `chost-m6.4` — Implement document backstage and recovery commands
163. `infra-m3.2` — Prove the unified workbook engine milestone
164. `chost-m4.8` — Expose application-level recalc and numeric preferences
165. `ceng-m7.1` — Retain the reference strategy and differential-twin harness
166. `dnacalc-m6.15` — Implement sort AutoFilter and advanced-filter UI
167. `dnacalc-m6.14` — Implement Find Replace Go To and Go To Special
168. `dnacalc-m6.13` — Implement data-validation dialog dropdowns and prompts
169. `dnacalc-m6.20` — Implement comments notes hyperlinks and protection UI
170. `dnacalc-m6.23` — Complete non-Excel skin rules tables and protected editing
171. `dnacalc-m6.7` — Implement freeze split and axis layout controls
172. `dnacalc-m6.9` — Implement Format Cells Font Border Fill and Protection
173. `dnacalc-m6.1` — Build ribbon backstage quick access and key tips
174. `infra-m4.1` — Prove edit recalc and save failure consistency
175. `chost-m5.1` — Expose iterative settings and what-if data-table commands
176. `chost-m4.10` — Coordinate multiple open documents and cross-book transactions
177. `dnacalc-m6.16` — Implement table creation and contextual table tools
178. `dnacalc-m6.10` — Implement cell styles themes and format painter
179. `dnacalc-m6.12` — Implement conditional-formatting UI and rule manager
180. `infra-m5.3` — Prove hard-calculation first-version coverage
181. `dnacalc-m6.18` — Implement calculation options and what-if table dialogs
182. `chost-m4.11` — Add events and exclusive batch execution mode
183. `infra-m7.2` — Generate and capture cross-feature workbook families
184. `chost-m4.12` — Expose object dispatch over host commands
185. `infra-m7.4` — Unify live and cached verdicts with raw numeric observations
186. `chost-m4.13` — Reserve add-in callback capabilities IDE panels and range picking
187. `chost-m6.3` — Implement formula audit trace and watch commands
188. `infra-m7.5` — Port one black-box smart-fuzzer campaign runner
189. `infra-m7.6` — Measure native target variation and software-Exact feasibility
190. `cfunc-m7.2` — Declare measured Fast-mode tolerance contracts
191. `chost-m8.1` — Complete application lifecycle and cross-skin recovery
192. `dnacalc-m6.19` — Implement Evaluate Formula trace watch and error tools
193. `dnacalc-m6.21` — Implement status aggregates zoom sheet tabs and window navigation
194. `infra-m7.7` — Verify portable Fast determinism and numeric mode defaults
195. `dnacalc-m6.22` — Complete keyboard routing accessibility and command parity
196. `onecalc-m8.1` — Verify the shipped OneCalc package stays engine-free
197. `dnacalc-m6.24` — Test original visual assets browser limits and input interoperability
198. `infra-m6.1` — Prove complete core UI and document authoring
199. `infra-m7.8` — Record named product-path benchmarks without thresholds
200. `infra-m7.9` — Fuzz operation sequences package parsing and cancellation
201. `infra-m7.10` — Publish conformance and campaign evidence through one report
202. `dnacalc-m8.1` — Integrate all skins and commands in the shipped packages
203. `infra-m8.1` — Verify core completion and mandatory phase-2 attachments
204. `infra-m8.2` — Enforce first-version release scope including separable areas
