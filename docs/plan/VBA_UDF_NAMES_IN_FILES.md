# VBA UDF names in Excel files

Measured for **dc-snz.7**, 2026-10-31: Excel **16.0 build 20527** (installed EXCEL.EXE **16.0.20527.20088**), `Windows (64-bit) NT 10.00`,
Windows 11 build 26200. Product source inspected and exercised at `c62ffca1e48b61989e7b481f9243a44fb6576b50`.
This is a file-format investigation; no product code was changed. Results describe this Excel build.

Evidence is in [`vba_udf_names/`](../../crates/calc-doc/tests/fixtures/vba_udf_names/):
small Excel-authored workbooks, their generating/measurement scripts, exported VBA modules,
`excel-observations.json`, `extra-observations.json`, `identifier-observations.json`,
`relocation-observations.json`, `prompt-observations.json`, `product-observations.json`,
`product-reopen.json`, `path-observations.json` and `excel-file-version.json`. `file-forms.json` contains the **exact XML cell
fragments, workbook and external-link XML, binary record offsets, complete record bytes, token bytes,
and SHA-256 of every retained package**. Names below identify entries in those files, not invented examples.
All VBA was written for this investigation under the repository MIT licence; no outside workbook/code was used.
Each script creates its own Excel instance, retries busy COM calls and closes only that instance.
PID 3144 and OxVba were not touched. No Trust Center or registry setting was changed.

## 1. Same-workbook functions: XML spelling is not a procedure inventory

`local.xlsm`, standard `Module1`, declares `Public Function MyFunc`, returning `x+100`.
These are exact saved fragments from `xl/worksheets/sheet1.xml` (each cell also has `cm="1"`):

```xml
<f t="array" ref="B1">MyFunc(2)</f>
<f t="array" ref="B2">Module1.MyFunc(2)</f>
<f t="array" ref="B3">[0]!MyFunc(2)</f>
<f t="array" ref="B4">[0]!Module1.MyFunc(2)</f>
```

All four recalculate to **102** after reopen. Typed `'local.xlsm'!MyFunc(2)` becomes
`local.xlsm!MyFunc(2)` in `Formula2`, but **`[0]!` in the file**; SaveAs changes the displayed workbook name.
Index 0 denotes this workbook; these local calls create no externalLink part. Module qualification survives.
`Formula2` entry gives these calls dynamic-array metadata even for one result;
the spelling conclusion does not depend on the result being an array.

There is **no normal `definedName` procedure-registration entry for MyFunc or its qualified spellings**.
`local.xlsm` has only `_xlpm.x` (`hidden="1" xlm="1"`, text `#NAME?`), `NamedLambda` and `SameName`.
In particular, VBA calls do not require XML `function`, `vbProcedure`, `xlm` or `functionGroupId` attributes.
The procedure inventory is in `xl/vbaProject.bin`, not recoverable from the normal defined-name table alone.

[ECMA-376 Part 1](https://ecma-international.org/publications-and-standards/standards/ecma-376/),
§18.2.5 / schema `CT_DefinedName`, defines those optional attributes (boolean defaults false).
Excel's own [MS-OI29500 §2.1.568](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/16c3c118-f358-493d-a99f-4c85ca834c00)
calls them reserved/unused; their existence in a schema does not establish that Excel writes a VBA symbol table there.
[MS-XLSX §2.2.2](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/3d025add-118d-4413-9856-ab65712ec1b0)
distinguishes name-based UDF calls, workbook-index prefixes and cell-function calls.

## 2. Other workbooks and VBA add-ins: the provider path matters

`provider.xlsm` defines `OtherFunc=x+2000`; `probe.xlam` defines `AddFunc=x+3000`.
The `.xlam` was loaded with `Workbooks.Open` (`IsAddin=True`), **without installing an add-in**.
`consumer.xlsx` and `consumer.xlsb` were saved with both providers open.

| Consumer cells | Exact saved XML `<f>` contents | Recalculated with providers open |
|---|---|---|
| B1 / B2 | `[1]!OtherFunc(2)` / `[1]!Module1.OtherFunc(2)` | 2002 / 2002 |
| B3 | `OtherFunc(2)` | `#NAME?`: another ordinary workbook does not supply an unqualified name |
| B4 / B5 | `[1]!HiddenOther(2)` / `[1]!PrivateOther(2)` | 2102 / 2202; see privacy below |
| B6 / B7 | `[2]!AddFunc(2)` / `[2]!AddFunc(2)` | 3002 / 3002; unqualified and explicitly qualified entry converge |
| B8 | `[2]!Module1.AddFunc(2)` | 3002 |

Both providers use **ordinary external-workbook links**, not the `_xll.` encoding.
Each externalLink lists procedure spellings as `<definedName name="OtherFunc"/>` or
`<definedName name="AddFunc"/>` inside `externalBook/definedNames`, without `refersTo` or VBA flags.
The sheet cache does not contain a UDF implementation. Workbook `externalReferences` supplies the link indexes.
For the add-in, these exact relationship attributes are saved in `externalLink2.xml.rels`:

```xml
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLinkPath" Target="probe.xlam" TargetMode="External"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLinkPath" Target="file:///C:\Work\DnaCalc\DnaCalc-cdoc\crates\calc-doc\tests\fixtures\vba_udf_names\probe.xlam" TargetMode="External"/>
```

`externalBook r:id="rId1"` also contains
`<xxl21:alternateUrls><xxl21:absoluteUrl r:id="rId2"/></xxl21:alternateUrls>`.
The provider link has the corresponding `provider.xlsm` paths. These are measured paths, not lookup suggestions.
[MS-XLSX §2.6.228](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-xlsx/cd7a81dc-3673-4acb-aefb-c7f02ffd9a88)
specifies the additional absolute/relative targets; the binary equivalent is
[BrtExternalLinksAlternateUrls](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xlsb/833f2664-28e4-49b9-8501-8d9d3f84fb07), type 5108.
Its measured payload in each consumer external-link part is
`00 00 00 00 00 00 00 00 04 00 00 00 72 00 49 00 64 00 32 00 00 00 00 00` (absolute relationship rId2).

Reopen results: original-provider controls in `measure.py` use `consumer.xlsx`; `relocate.py` repeats
the missing/moved controls for **both XML and binary inputs**. Explicit ChangeLink was measured on XLSX.

| Provider state / open policy | After `CalculateFullRebuild` | Saved link / repair |
|---|---|---|
| Neither provider open; original files still exist; `UpdateLinks=0` or `3` | All qualified calls `#NAME?` | Paths retained; updating links did not load/execute the UDF provider |
| Only original workbook / only original add-in open | Only that provider's calls work | No path change |
| Both original providers open | Original results return | No path change |
| Add-in moved to `moved/probe.xlam`, absent from original location | Add-in calls `#NAME?` | Old path remains |
| Same basename loaded from `moved/`, before consumer opens; `UpdateLinks=0` or `3` | Still `#NAME?` | **No automatic basename rebinding or path repair** |
| Explicit `ChangeLink` to the moved, loaded add-in | Add-in calls return 3002 | Relative target becomes `moved/probe.xlam`; absolute target changes too |

Before forced recalculation, saved 2002/3002 caches can still be displayed: those are not successful executions.
With alerts and `AskToUpdateLinks` enabled and the add-in missing, Excel displayed the ordinary
external-source warning, offering **Update / Don't Update**. Selecting Update returned to the workbook;
forced recalculation gave `#NAME?`. No file-repair or choose-a-replacement dialog appeared in that run.
The silent runs deliberately suppressed alerts; they establish no general rule about every interactive configuration.

## 3. Unknown functions, prefixes, disabled macros and macro-free saves

`local.xlsm` B5–B8 and B10/B14 separate typed entry from **loading file syntax**:

| Typed / saved formula contents | On immediate entry | On XML reopen and recalculation |
|---|---|---|
| `NotThere(2)` | `#NAME?` | Same display, `#NAME?`; Excel did **not** add `_xludf.` |
| `_xludf.NotThere(2)` | `#NAME?` | Display `NotThere(2)`, still `#NAME?`; resave drops the unnecessary prefix |
| `_xlfn.NotThere(2)` | `#NAME?` | Prefix remains visible; still `#NAME?` |
| `_xll.NotThere(2)` | `#NAME?` | Display `NotThere(2)`; still `#NAME?`; binary resave uses an add-in placeholder |
| `_xludf.SUM(2)` / `_xludf.ACOT(2)` | Both `#NAME?` | Display `SUM(2)` / `ACOT(2)`, execute the **VBA** functions: 202 / 302 |
| `SUM(2)` / `ACOT(2)` | Built-ins: 2 / 0.4636476090008061 | Built-ins remain; file stores `SUM(2)` / `_xlfn.ACOT(2)` |

`Module1.SUM(2)` and `Module1.ACOT(2)` execute VBA immediately (202/302).
Excel rejected workbook-only `'local.xlsm'!SUM(2)` and `'local.xlsm'!ACOT(2)` with COM error 1004;
workbook-plus-module qualification works for ordinary MyFunc. Do not infer one rule for all qualified built-ins.
`_xludf.` distinguishes a file's forced UDF call from a built-in collision; **it is not a prefix for every VBA call**.
The prefix behavior above is measured; the cited MS-XLSX grammar does not itself explain all these load-time rewrites.

`local.xlsb`, saved directly after literal prefixed entry, retains literal `_xludf.SUM` names with flags 0:
its reopen still gives `#NAME?`. Opening the XML first and then saving `loaded-xml.xlsb` instead creates
bare `SUM` / `ACOT` VBA name records (flags `0x0000000e`), and they execute as VBA.
Thus converting text or adding/removing a prefix without knowing the source representation changes meaning.

For the disabled-macro probe, only that automation instance's **per-open** `AutomationSecurity=3` policy
was used, then restored. Cached 102 could be observed before recalculation; recalculation gave `#NAME?`.
`disabled-resaved.xlsm` keeps VBA and UDF formula spellings. `stripped.xlsx` (SaveAs format 51) has no
VBA project but keeps UDF calls, including `[0]!` and `_xludf.`. Its fresh recalculation gives `#NAME?`
for those UDFs, while its LAMBDA still returns 1402. Its original saved 102 cache is not evidence of surviving VBA.

## 4. Name boundaries, case, visibility, names and function help

| Case (local cell unless stated) | Excel observation and retained file form |
|---|---|
| Underscores / digits | Declared `Foo_2`, typed and saved `foo_2(2)`, gives 402. Digit-first `2Foo(2)` entry is rejected. |
| Non-ASCII | Declared `Café` / `Ångström`; entering `café`, `CAFÉ`, `ångström`, `ÅNGSTRÖM` all executes. Subsequent uppercase entries reuse the existing name-table spelling: B38/B39 display/save `café`/`ångström`. Exported `.bas` files use the project's Windows code page, unlike XML's Unicode. |
| A1 / XFD1 / R1C1 | Declarations are accepted. Bare `A1(2)` / `XFD1(2)` give `#REF!`; `R1C1(2)` is displayed/saved `$A$1(2)`. `Module1.A1`, `.XFD1`, `.R1C1` execute (702/802/902). These are cell-call syntax collisions, not simply forbidden procedure names. |
| Dots / initial underscore | `Foo.Bar(2)` and `_Leading(2)` save unchanged and give `#NAME?`. Injected declarations `Foo.Bar`, `_Leading`, `2Foo` make a control macro unrunnable (1004); `ValidName` control runs. Do not treat dots in module qualification as part of a VBA procedure identifier. |
| Declared / typed case | Declared `CaseFunc`, first typed `cAsEfUnC`: saved spelling remains `cAsEfUnC`, including the binary name record. Changing declaration case to `CASEFUNC` preserves spelling and execution. Existing name-table spelling can win over later typed case; declaration case alone does not determine file case. |
| Procedure rename | Changing declaration/body to `RenamedFunc` does **not** rewrite `cAsEfUnC(2)`: old call becomes `#NAME?`. New `renamedfunc(2)` works; `renamed.xlsm` keeps both names. |
| `Private Function` | `SecretFunc(2)` and `Module1.SecretFunc(2)` execute from cells. A qualified call from another workbook to its provider's private `HiddenOther` also executes. These observations are worksheet lookup behavior, not a claim about VBA-to-VBA visibility. |
| `Option Private Module` | Local `HiddenFunc(2)` and external `provider.xlsm!PrivateOther(2)` execute. XML supplies no extra visibility attributes for these calls. |
| Defined name and UDF both `SameName` | Defined name `42` wins: `SameName` is 42, `SameName(2)` gives `#VALUE!`. `Module1.SameName(2)` executes VBA and gives 1102. |
| Named LAMBDA contrast | `<definedName name="NamedLambda">_xlfn.LAMBDA(_xlpm.x,_xlpm.x+1400)</definedName>`; cell `NamedLambda(2)` gives 1402 without VBA, including in `.xlsx`. It is an actual formula definition, unlike the VBA names absent from normal XML definedNames. |

MacroOptions measurements (`metadata-vba.*`, `metadata-custom-vba.*`): a setter executed through COM's
`Application.Run` calls `Application.MacroOptions` in VBA, with category 3 or custom `DNA UDF custom`.
The persisted module source contains exactly `Attribute MetaFunc.VB_Description = "VBA-set description"`
and `Attribute MetaFunc.VB_ProcData.VB_Invoke_Func = " \n3"`; the custom probe has description
`"Custom category description"` and `" \n20"`. Here `\n` is two literal characters in the attribute string.
The custom label is in workbook XML `<functionGroups builtInGroupCount="19"><functionGroup name="DNA UDF custom"/></functionGroups>`.
Argument help is in workbook `extLst`, URI `{8193D73E-4604-4355-AFF4-8A2333D13BE4}`, namespace
`http://schemas.microsoft.com/office/spreadsheetml/2009/9/main`, with this exact nested XML:

```xml
<x14:definedNames><x14:definedName name="MetaFunc"><x14:argumentDescriptions count="1"><x14:argumentDescription index="0">VBA-set argument</x14:argumentDescription></x14:argumentDescriptions></x14:definedName></x14:definedNames>
```

Those extension names are separate from normal `definedNames`. Direct COM calls with omitted optional
arguments in the earlier controls changed description but did not establish category/argument-help changes;
the explicit VBA setters above are the evidence for their persistence. Source attributes survive reopen.
[MS-OVBA §2.3.4.3](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-ovba/c66b58a6-f8ba-4141-9382-0612abce9926)
specifies module source as compressed, project-code-page text, separate from implementation-specific performance caches.
[MacroOptions](https://learn.microsoft.com/en-us/office/vba/api/excel.application.macrooptions) documents the metadata inputs;
[MS-XLSB BrtNameExt](https://learn.microsoft.com/zh-cn/openspecs/office_file_formats/ms-xlsb/6af28539-9af5-4ec3-b80f-f63fe7b238a5)
specifies the binary argument-description record. Custom category numbers are not portable fixed identifiers.

## 5. Binary evidence: names, tokens and metadata

`local.xlsb`: MyFunc is **name index 24**, module spelling index 18; both have flags `0x0000000e`,
scope `ffffffff`, empty name formula. Complete MyFunc `BrtName` record (type/size header included):

```text
27 25 0e 00 00 00 00 ff ff ff ff 06 00 00 00 4d 00 79 00 46 00 75 00 6e 00 63 00 00 00 00 00 00 00 00 00 ff ff ff ff
```

Flags set `fFunc`, `fOB`, `fProc`; do not replace these observed bits with an assumption that only fOB is set.
The corresponding `BrtArrFmla` rgce bytes (complete surrounding records/offsets are in `file-forms.json`):

| File / cell | Exact rgce bytes | Meaning |
|---|---|---|
| local.xlsb B1 | `23 18 00 00 00 1e 02 00 42 02 ff 00` | PtgName 24, integer 2, function 255 with two operands including the name |
| local.xlsb B2 | `23 12 00 00 00 1e 02 00 42 02 ff 00` | Module-qualified name 18 |
| local.xlsb B3 | `39 00 00 18 00 00 00 1e 02 00 42 02 ff 00` | PtgNameX, own-workbook supporting link |
| consumer.xlsb B1 | `39 00 00 03 00 00 00 1e 02 00 42 02 ff 00` | External link XTI 0, external name 3 OtherFunc |
| consumer.xlsb B6 | `39 01 00 01 00 00 00 1e 02 00 42 02 ff 00` | External link XTI 1, external name 1 AddFunc |

External parts have `BrtBeginSupBook` payload `00 00 04 00 00 00 72 00 49 00 64 00 31 00 ff ff ff ff`
(workbook kind, rId1); each procedure's `BrtSupNameBits` payload is `40 00 00 00 00 00 00`.
This does not identify VBA visibility; some of those bits are unspecified/unused in the published structure.
`metadata-unqualified.xlsb` has BrtNameExt **1036**, header `8c 08 44`, payload beginning
`00 00 00 00 01 00 00 00 08 00 00 00 4d 00 65 00 74 00 61 00 46 00 75 00 6e 00 63 00`;
the remaining bytes encode argument index 0 and `VBA-set argument`. Custom categories use BrtFnGroup 665.
`loaded-xml.xlsb` instead uses BrtSupAddin 667 + BrtPlaceholderName 361 for the missing `_xll.NotThere` control.

Interpretation: [MS-XLSB BrtName](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xlsb/10be4364-6518-41fe-af17-ffe0149d147d),
[PtgFuncVar](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xlsb/09e39491-a417-49d8-93f4-712fc3de7a49),
[Rgce](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xlsb/54897b6b-9f69-4c17-868a-b09ef126c8ab),
[PtgNameX](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xlsb/c7ff7b0e-21bb-46ac-8f86-daf2a0a22326),
and [ExternalNameProperties](https://learn.microsoft.com/mt-mt/openspecs/office_file_formats/ms-xlsb/51e684f3-e5db-40e9-ad90-56f81127cb55).

## 6. DNA Calc today and required changes

The native `probe.rs` uses the existing fixture helper and actual open/load/prepare_edit/prepare_save/write APIs;
its binding probes use the public compiler with explicit registrations, not a replacement evaluator.
Excel reopens the actual saved outputs. Loading/retaining a cached result is never counted as calculation.

| Surface | Current behavior, evidence and owning work |
|---|---|
| XML load/save, all measured call spellings | Raw file-form text loads and survives untouched save and an unrelated value edit, including unknown calls, qualifiers, case and VBA bytes. `crates/calc-doc/src/load/sheet.rs:253` loads formula text; `src/save/author.rs:183` changes only authored formulas; `src/save/mod.rs:1` copies other parts. No blanket UDF-formula-loss claim is supported. |
| XML name attributes | `crates/calc-doc/src/load/mod.rs:934` reads normal names and the three macro flags; `crates/calc-doc-model/src/definitions.rs:68` has no functionGroupId/description/category model. Opaque source XML survives saves; absent model fields alone do not establish file loss. |
| Binary calls | `crates/calc-doc/src/xlsb/formula.rs:714` resolves external/own-book/add-in names; `:760` handles function-255 call spelling and built-in collisions. `src/xlsb/workbook.rs:450` correctly omits empty UDF-name formulas from normal XML. Measured local and same-folder external calls survive conversion. |
| Binary output format | `crates/calc-doc/src/xlsb/mod.rs:1` converts at open; saves emit XML `.xlsm`/`.xlsx`, not `.xlsb`. **dc-snz.14 P2** requests actual binary writing; current conversion is an explicit existing limitation. |
| Authored forced-UDF LAMBDA | `crates/calc-doc/src/save/file_form.rs:495` treats `_xludf.LAMBDA` as language LAMBDA and `:614` inserts parameter prefixes. Re-entering `_xludf.LAMBDA(Rate,3)` writes `_xludf.LAMBDA(_xlpm.Rate,3)`. Excel: original 5005, saved copy `#VALUE!`, for both input formats. **dc-snz.8 P1**: preserve UDF identity before rewriting parameters. Retained `dna-collision-reentered.xlsm` demonstrates it. |
| External link location | `crates/calc-doc/src/xlsb/parts.rs:656` drops alternate URLs when rebuilding externalBook. Same-folder saved consumer works; output in another folder gives qualified calls `#NAME?` despite original providers loaded. XML-input output and Excel's own binary SaveAs work there. **dc-snz.12 P1**: retain absolute fallback/provider identity and handle destination-relative paths. |
| Binary function-help metadata | `crates/calc-doc/src/xlsb/workbook.rs:261` ignores BrtNameExt and function-group records. Converted metadata fixtures retain source description/category ID in VBA but lose argument-description XML/custom category label. **dc-snz.13 P2**. XML-input opaque metadata is preserved. |
| UserCall binding / qualification | `crates/calc-fml/src/eval/compile.rs:990` emits ASCII-uppercase UserCall only after exact registry/name lookup; MyFunc and separately registered Module1.MyFunc work. `[0]!MyFunc`, file-name-qualified and external-index calls fail with `QualifiedNonReference` (`:471`). **dc-snz.9 P2**: bind workbook/module/provider identity. |
| Forced-UDF prefixes | `crates/calc-fml/src/eval/compile.rs:2260` strips only `_xlfn.`/`_xlws.`; `_xludf.SUM`/`ACOT`/`MyFunc` emit no UserCall with those bare procedures registered. **dc-snz.10 P2**: distinguish loaded forced-UDF identity from built-ins and interactive literal entry. |
| Non-ASCII case | `crates/calc-func/src/udf.rs:63`/`:72` use ASCII uppercase. Registered Café does not match CAFÉ; Ångström does not match ångström. **dc-snz.11 P2**: use shared Excel-compatible comparison. Plain ASCII case already works. |
| Project/visibility discovery | Codec carries VBA opaquely (`crates/calc-doc/src/xlsb/mod.rs:9`); normal XML names cannot supply registrations, module or Private/Option Private behavior. **dc-snz.15 P2** owns project ingestion; existing **dc-snz.6** owns runtime registration/recalculation. LAMBDA/name precedence remains on the existing formula route. |

Validation: Windows `cargo xtask check` passed: 347 passed, 0 failed, 0 ignored, 1 not run in 28 harnesses of four packages; read-only review verified package bytes and both P1 controls.
These are measured witnesses, not a UDF conformance pass-rate claim. Remaining breadth includes installed-add-in
startup policies, other Excel builds/locales and arbitrary path-recovery UI configurations; no result here proves them.

## Follow-up fixes (dc-snz.8 and dc-snz.12)

The authored-save regression test now preserves the ordinary `Rate` argument of the forced
UDF call while retaining language LAMBDA parameter prefixes. The XLSB conversion retains
`BrtExternalLinksAlternateUrls` and the workbook folder from `BrtAbsPath15` (2071), written
as `x15ac:absPath`. The latter is essential on this Excel build: preserving alternate URLs
alone still gave `#NAME?` after relocation. Excel controls restoring the original folder,
with and without alternate URLs, both restored the original provider calls.
Evidence: `cdoc-path-diagnostic.json`, `cdoc-p1-reopen.json` and `tests/vba_udf_names.rs` in
the existing fixture/test directories. These are Excel file-save/reopen checks, not claims
that DNA Calc executes the VBA. Format writing remains XML.

For dc-snz.9, CalcFml's registry compile/evaluate route now binds the measured own-workbook
and external workbook/module calls. Plans retain link indexes, procedure spelling (including
module qualification), and the exact provider target. CalcDoc loads main and absolute targets
into the model; CalcEngine's catalogue resolves those targets without choosing an ambiguous
basename. The fixture tests use an identity adapter to verify binding; they do not execute
the fixture VBA or score file caches. CalcHost/CalcEngine registry execution remains dc-snz.6.
Host integration must use the provider-scoped `registrations()` inventory and the actual
workbook location. Without that location, binding currently uses the retained absolute
fallback; moving consumer and provider together has not been measured here.

For dc-snz.11, registry registration, lookup, duplicate detection and invocation now use
`calc-value`'s shared workbook-name key. Plans use that same key instead of a separate ASCII
case policy; the stored formula spelling stays unchanged. The regression covers typed café/CAFÉ and
ångström/ÅNGSTRÖM and the actual saved spellings from three measured XML/binary packages.

For dc-snz.13, conversion now writes custom function categories and argument help from the
binary records into `functionGroups` and the x14 defined-name extension. Untouched and
unrelated-value-edit saves match the measured XML subtrees and retain the original VBA bytes.
`cdoc-metadata-reopen.json` records Excel reopening the corrected files, calculating MetaFunc,
and saving the same binary category/help payloads; exported description/category attributes
also match. These properties remain preserved file metadata, without new fields in the normal
defined-name model or invented procedure registrations. The corrected function-help dialog
was not inspected: Computer Use was stopped with the physical Escape key after the original
dialog was observed; the owned inspection instance was closed.

For dc-snz.14, `SavePolicy.format = SaveFormat::Xlsb` now preserves the validated original
binary package through the existing ZIP writer when no parts need rewriting, including the
actual Host save route. Tests compare every uncompressed part of four measured XLSB inputs,
including VBA, tokens, name flags and links, and check foreign models, size bounds and changed
backing sources. A request requiring part rewrites returns a typed dc-snz.14 refusal; default
output remains XML. The original package backing is retained along with the converted view.
This is a partial binary save capability: binary authoring still needs the writer and the
catalogue's function-index/prefix/call-identity data; no new function-name table was added.

For dc-snz.15, `LoadedWorkbook::read_vba_project` explicitly reads the workbook's related
project without compiling, registering or executing VBA. It retains module and stream names,
the directory's module kind/private/read-only flags, help metadata, the project code page,
raw source bytes and strictly decoded source text, including exported Attribute lines.
Tests compare the measured XML/binary/disabled-project sources byte for byte with the Excel
exports, including accented identifiers and function attributes. It uses cfb 0.10.0 for storage,
nom 7.1.3 for a directory parser ported from ovba 0.7.1 (MIT, notice retained), and codepage
0.1.3 for encoding labels. The port replaces upstream panic paths with fallible strict decoding
and full-input parsing; compressed directory/source streams have checked chunk/copy/output
bounds. Tests cover every truncation of the measured directory, trailing bytes and unknown
code pages. Normal open/save remains
opaque and inert. This is a partial source-ingestion step: callable procedure discovery,
accessibility and project admission through the existing VBA frontend/Host remain open.

