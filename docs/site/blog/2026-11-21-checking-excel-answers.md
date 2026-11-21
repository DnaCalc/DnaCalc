# How DNA Calc checks formulas against Excel
Tags: Excel parity, testing, function bank
Status: draft

Excel workbooks often contain saved results alongside their formulas. When DNA Calc opens a book,
it can show those values with file-cache provenance. A comparison counts a match only after the
engine has actually calculated the result and the verdict has checked it against Excel's answer.

The Function Bank tests individual worksheet functions across many inputs. Whole-workbook
comparisons also cover cell references, dependency order and workbook settings. The reports carry
matched, mismatched, refused and missing results. The pass rate uses every formula cell in the
comparison.

In the video, start with a formula that spills several values and then change one input to an
error. Look at the value, type and spill shape. A display rounded to a few digits can conceal a
difference that the verdict will still record.
