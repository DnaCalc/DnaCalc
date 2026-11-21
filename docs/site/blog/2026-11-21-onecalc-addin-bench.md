# Test an Excel-DNA add-in with OneCalc
Tags: OneCalc, Windows, add-ins
Status: draft

Open DNA OneCalc for Windows and load your own Excel-DNA XLL in the Add-ins panel. Type a formula
that calls one exported function. Start with a scalar input whose expected result you know.
Change that input and repeat with an error or a blank to see how the function handles it.

For the video, use the DnaSimple reference add-in and call `=SimpleNeg(2.5)`. Enter the same formula
in Excel and compare the returned value and type. If the call reaches a host service DNA OneCalc
cannot provide, the result can carry a refusal that names the missing behavior.

The same exercise works as a small diagnostic for a function that has several entry paths.
Keep the function, arguments and Excel build with the result so somebody else can reproduce it.
